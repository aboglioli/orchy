use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{ContextSnapshot, ContextStore, SnapshotId};
use orchy_core::namespace::Namespace;

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

impl ContextStore for SqliteBackend {
    async fn save(&self, snapshot: &ContextSnapshot) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let embedding_bytes = snapshot.embedding().map(|e| embedding_to_bytes(e));

        conn.execute(
            "INSERT OR REPLACE INTO contexts (id, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                snapshot.id().to_string(),
                snapshot.agent_id().to_string(),
                snapshot.namespace().to_string(),
                snapshot.summary(),
                embedding_bytes,
                snapshot.embedding_model(),
                snapshot.embedding_dimensions().map(|d| d as i64),
                serde_json::to_string(snapshot.metadata()).unwrap(),
                snapshot.created_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let rowid = conn.last_insert_rowid();

        conn.execute(
            "INSERT INTO contexts_fts(rowid, namespace, summary) VALUES(?1, ?2, ?3)",
            rusqlite::params![rowid, snapshot.namespace().to_string(), snapshot.summary()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(ref emb_bytes) = embedding_bytes {
            let _ = conn.execute(
                "INSERT INTO contexts_vec(rowid, embedding) VALUES(?1, ?2)",
                rusqlite::params![rowid, emb_bytes],
            );
        }

        Ok(())
    }

    async fn find_latest(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at
                 FROM contexts WHERE agent_id = ?1
                 ORDER BY created_at DESC LIMIT 1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![agent.to_string()], row_to_context)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts WHERE (namespace = ?1 OR namespace LIKE ?1 || '/%')",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        params.push(Box::new(namespace.to_string()));
        let mut idx = 2;

        if let Some(a) = agent {
            sql.push_str(&format!(" AND agent_id = ?{idx}"));
            params.push(Box::new(a.to_string()));
            idx += 1;
        }
        let _ = idx;

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let snapshots = stmt
            .query_map(param_refs.as_slice(), row_to_context)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(snapshots)
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT c.id, c.agent_id, c.namespace, c.summary, c.embedding, c.embedding_model, c.embedding_dimensions, c.metadata, c.created_at
             FROM contexts c
             JOIN contexts_fts ON contexts_fts.rowid = c.rowid
             WHERE contexts_fts MATCH ?1
             AND (c.namespace = ?2 OR c.namespace LIKE ?2 || '/%')",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let fts_query = sanitize_fts_query(query);
        params.push(Box::new(fts_query));
        params.push(Box::new(namespace.to_string()));
        let mut idx = 3;

        if let Some(a) = agent_id {
            sql.push_str(&format!(" AND c.agent_id = ?{idx}"));
            params.push(Box::new(a.to_string()));
            idx += 1;
        }

        sql.push_str(&format!(" ORDER BY rank LIMIT ?{idx}"));
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let snapshots = stmt
            .query_map(param_refs.as_slice(), row_to_context)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(snapshots)
    }
}

fn row_to_context(row: &rusqlite::Row) -> rusqlite::Result<ContextSnapshot> {
    let id_str: String = row.get(0)?;
    let agent_id_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let summary: String = row.get(3)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(4)?;
    let embedding_model: Option<String> = row.get(5)?;
    let embedding_dimensions: Option<i64> = row.get(6)?;
    let metadata_str: String = row.get(7)?;
    let created_at_str: String = row.get(8)?;

    let id = SnapshotId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let agent_id = AgentId::from_str(&agent_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(ContextSnapshot::restore(
        id,
        agent_id,
        namespace,
        summary,
        embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions.map(|d| d as u32),
        serde_json::from_str(&metadata_str).unwrap_or_else(|_| HashMap::new()),
        created_at,
    ))
}

fn sanitize_fts_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            let escaped = word.replace('"', "\"\"");
            format!("\"{escaped}\"")
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}
