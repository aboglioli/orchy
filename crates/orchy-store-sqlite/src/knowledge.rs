use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeKind, KnowledgeStore, RestoreKnowledge,
    Version,
};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

impl KnowledgeStore for SqliteBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

            let embedding_bytes = entry.embedding().map(embedding_to_bytes);
            let tags_json =
                serde_json::to_string(entry.tags()).map_err(|e| Error::Store(e.to_string()))?;
            let metadata_json =
                serde_json::to_string(entry.metadata()).map_err(|e| Error::Store(e.to_string()))?;

            conn.execute(
                "INSERT OR REPLACE INTO knowledge_entries (id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                rusqlite::params![
                    entry.id().to_string(),
                    entry.project().to_string(),
                    entry.namespace().to_string(),
                    entry.path(),
                    entry.kind().to_string(),
                    entry.title(),
                    entry.content(),
                    tags_json,
                    entry.version().as_u64() as i64,
                    entry.agent_id().map(|a| a.to_string()),
                    metadata_json,
                    embedding_bytes,
                    entry.embedding_model(),
                    entry.embedding_dimensions().map(|d| d as i64),
                    entry.created_at().to_rfc3339(),
                    entry.updated_at().to_rfc3339(),
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = entry.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                 FROM knowledge_entries WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_entry)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                 FROM knowledge_entries WHERE project = ?1 AND namespace = ?2 AND path = ?3",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![project.to_string(), namespace.to_string(), path],
                row_to_entry,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at FROM knowledge_entries WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref project) = filter.project {
            sql.push_str(&format!(" AND project = ?{idx}"));
            params.push(Box::new(project.to_string()));
            idx += 1;
        }
        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
                ));
                params.push(Box::new(ns.to_string()));
                idx += 1;
            }
        }
        if let Some(ref kind) = filter.kind {
            sql.push_str(&format!(" AND kind = ?{idx}"));
            params.push(Box::new(kind.to_string()));
            idx += 1;
        }
        if let Some(ref tag) = filter.tag {
            sql.push_str(&format!(" AND tags LIKE ?{idx}"));
            params.push(Box::new(format!("%\"{tag}\"%")));
            idx += 1;
        }
        if let Some(ref prefix) = filter.path_prefix {
            sql.push_str(&format!(" AND path LIKE ?{idx}"));
            params.push(Box::new(format!("{prefix}%")));
            idx += 1;
        }
        if let Some(ref agent_id) = filter.agent_id {
            sql.push_str(&format!(" AND agent_id = ?{idx}"));
            params.push(Box::new(agent_id.to_string()));
            idx += 1;
        }

        let _ = idx;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_entry)
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
    ) -> Result<Vec<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT e.id, e.project, e.namespace, e.path, e.kind, e.title, e.content, e.tags, e.version, e.agent_id, e.metadata, e.embedding, e.embedding_model, e.embedding_dimensions, e.created_at, e.updated_at
             FROM knowledge_entries e
             WHERE (e.title LIKE ?1 OR e.content LIKE ?1 OR e.path LIKE ?1)",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let like_query = format!("%{}%", query);
        params.push(Box::new(like_query));
        let mut idx = 2;

        if let Some(ns) = namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (e.namespace = ?{idx} OR e.namespace LIKE ?{idx} || '/%')"
                ));
                params.push(Box::new(ns.to_string()));
                idx += 1;
            }
        }

        sql.push_str(&format!(" LIMIT ?{idx}"));
        params.push(Box::new(limit as i64));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let entries = stmt
            .query_map(param_refs.as_slice(), row_to_entry)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(entries)
    }

    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM knowledge_entries WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Knowledge> {
    let id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let path: String = row.get(3)?;
    let kind_str: String = row.get(4)?;
    let title: String = row.get(5)?;
    let content: String = row.get(6)?;
    let tags_json: String = row.get(7)?;
    let version: i64 = row.get(8)?;
    let agent_id_str: Option<String> = row.get(9)?;
    let metadata_json: String = row.get(10)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(11)?;
    let embedding_model: Option<String> = row.get(12)?;
    let embedding_dimensions: Option<i64> = row.get(13)?;
    let created_at_str: String = row.get(14)?;
    let updated_at_str: String = row.get(15)?;

    let id = KnowledgeId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let kind = KnowledgeKind::from_str(&kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            4,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(7, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let metadata: HashMap<String, String> = serde_json::from_str(&metadata_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Knowledge::restore(RestoreKnowledge {
        id,
        project,
        namespace,
        path,
        kind,
        title,
        content,
        tags,
        version: Version::from(version as u64),
        agent_id: agent_id_str.and_then(|s| AgentId::from_str(&s).ok()),
        metadata,
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        created_at,
        updated_at,
    }))
}
