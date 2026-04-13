use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore, Version};
use orchy_core::namespace::Namespace;

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

impl MemoryStore for SqliteBackend {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let embedding_bytes = entry.embedding().map(embedding_to_bytes);

        conn.execute(
            "INSERT OR REPLACE INTO memory (namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                entry.namespace().to_string(),
                entry.key(),
                entry.value(),
                entry.version().as_u64() as i64,
                embedding_bytes,
                entry.embedding_model(),
                entry.embedding_dimensions().map(|d| d as i64),
                entry.written_by().map(|a| a.to_string()),
                entry.created_at().to_rfc3339(),
                entry.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let rowid = conn.last_insert_rowid();

        // Rebuild FTS entry
        let _ = conn.execute(
            "INSERT INTO memory_fts(memory_fts, rowid, namespace, key, value) VALUES('delete', ?1, ?2, ?3, ?4)",
            rusqlite::params![rowid, entry.namespace().to_string(), entry.key(), entry.value()],
        );
        conn.execute(
            "INSERT INTO memory_fts(rowid, namespace, key, value) VALUES(?1, ?2, ?3, ?4)",
            rusqlite::params![
                rowid,
                entry.namespace().to_string(),
                entry.key(),
                entry.value()
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(ref emb_bytes) = embedding_bytes {
            let _ = conn.execute(
                "DELETE FROM memory_vec WHERE rowid = ?1",
                rusqlite::params![rowid],
            );
            let _ = conn.execute(
                "INSERT INTO memory_vec(rowid, embedding) VALUES(?1, ?2)",
                rusqlite::params![rowid, emb_bytes],
            );
        }

        Ok(())
    }

    async fn find_by_key(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at
                 FROM memory WHERE namespace = ?1 AND key = ?2",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![namespace.to_string(), key], row_to_memory)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = "SELECT namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at FROM memory WHERE 1=1".to_string();
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(project.to_string()));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_memory)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(entries)
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT m.namespace, m.key, m.value, m.version, m.embedding, m.embedding_model, m.embedding_dimensions, m.written_by, m.created_at, m.updated_at
             FROM memory m
             JOIN memory_fts ON memory_fts.rowid = m.rowid
             WHERE memory_fts MATCH ?1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let fts_query = sanitize_fts_query(query);
        params.push(Box::new(fts_query));
        let mut idx = 2;

        if let Some(ns) = namespace {
            sql.push_str(&format!(
                " AND (m.namespace = ?{idx} OR m.namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }

        sql.push_str(&format!(" ORDER BY rank LIMIT ?{idx}"));
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_memory)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(entries)
    }

    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM memory WHERE namespace = ?1 AND key = ?2",
                rusqlite::params![namespace.to_string(), key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(rowid) = rowid {
            let _ = conn.execute(
                "INSERT INTO memory_fts(memory_fts, rowid, namespace, key, value) VALUES('delete', ?1, ?2, ?3, (SELECT value FROM memory WHERE namespace = ?2 AND key = ?3))",
                rusqlite::params![rowid, namespace.to_string(), key],
            );

            let _ = conn.execute(
                "DELETE FROM memory_vec WHERE rowid = ?1",
                rusqlite::params![rowid],
            );
        }

        conn.execute(
            "DELETE FROM memory WHERE namespace = ?1 AND key = ?2",
            rusqlite::params![namespace.to_string(), key],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryEntry> {
    let namespace_str: String = row.get(0)?;
    let key: String = row.get(1)?;
    let value: String = row.get(2)?;
    let version: i64 = row.get(3)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(4)?;
    let embedding_model: Option<String> = row.get(5)?;
    let embedding_dimensions: Option<i64> = row.get(6)?;
    let written_by_str: Option<String> = row.get(7)?;
    let created_at_str: String = row.get(8)?;
    let updated_at_str: String = row.get(9)?;

    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(MemoryEntry::restore(
        namespace,
        key,
        value,
        Version::from(version as u64),
        embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions.map(|d| d as u32),
        written_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at,
        updated_at,
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
