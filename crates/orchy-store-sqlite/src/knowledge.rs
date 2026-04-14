use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use sea_query::{Cond, Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Entry, EntryFilter, EntryId, EntryStore, EntryType, RestoreEntry, Version,
};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

#[derive(Iden)]
enum Entries {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "path"]
    Path,
    #[iden = "entry_type"]
    EntryType,
    #[iden = "title"]
    Title,
    #[iden = "content"]
    Content,
    #[iden = "tags"]
    Tags,
    #[iden = "version"]
    Version,
    #[iden = "agent_id"]
    AgentId,
    #[iden = "metadata"]
    Metadata,
    #[iden = "embedding"]
    Embedding,
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

impl EntryStore for SqliteBackend {
    async fn save(&self, entry: &mut Entry) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

            let embedding_bytes = entry.embedding().map(embedding_to_bytes);
            let tags_json =
                serde_json::to_string(entry.tags()).map_err(|e| Error::Store(e.to_string()))?;
            let metadata_json =
                serde_json::to_string(entry.metadata()).map_err(|e| Error::Store(e.to_string()))?;

            conn.execute(
                "INSERT OR REPLACE INTO entries (id, project, namespace, path, entry_type, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                rusqlite::params![
                    entry.id().to_string(),
                    entry.project().to_string(),
                    entry.namespace().to_string(),
                    entry.path(),
                    entry.entry_type().to_string(),
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

    async fn find_by_id(&self, id: &EntryId) -> Result<Option<Entry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, entry_type, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                 FROM entries WHERE id = ?1",
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
    ) -> Result<Option<Entry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, entry_type, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                 FROM entries WHERE project = ?1 AND namespace = ?2 AND path = ?3",
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

    async fn list(&self, filter: EntryFilter) -> Result<Vec<Entry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut query = Query::select();
        query.from(Entries::Table).columns([
            Entries::Id,
            Entries::Project,
            Entries::Namespace,
            Entries::Path,
            Entries::EntryType,
            Entries::Title,
            Entries::Content,
            Entries::Tags,
            Entries::Version,
            Entries::AgentId,
            Entries::Metadata,
            Entries::Embedding,
            Entries::EmbeddingModel,
            Entries::EmbeddingDimensions,
            Entries::CreatedAt,
            Entries::UpdatedAt,
        ]);

        if let Some(ref project) = filter.project {
            query.and_where(Expr::col(Entries::Project).eq(project.to_string()));
        }
        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                query.cond_where(
                    Cond::any()
                        .add(Expr::col(Entries::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Entries::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref entry_type) = filter.entry_type {
            query.and_where(Expr::col(Entries::EntryType).eq(entry_type.to_string()));
        }
        if let Some(ref tag) = filter.tag {
            query.and_where(Expr::col(Entries::Tags).like(format!("%\"{}\"%%", tag)));
        }
        if let Some(ref prefix) = filter.path_prefix {
            query.and_where(Expr::col(Entries::Path).like(format!("{}%", prefix)));
        }
        if let Some(ref agent_id) = filter.agent_id {
            query.and_where(Expr::col(Entries::AgentId).eq(agent_id.to_string()));
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let entries = stmt
            .query_map(&*values.as_params(), row_to_entry)
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
    ) -> Result<Vec<Entry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT e.id, e.project, e.namespace, e.path, e.entry_type, e.title, e.content, e.tags, e.version, e.agent_id, e.metadata, e.embedding, e.embedding_model, e.embedding_dimensions, e.created_at, e.updated_at
             FROM entries e
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

    async fn delete(&self, id: &EntryId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM entries WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Entry> {
    let id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let path: String = row.get(3)?;
    let entry_type_str: String = row.get(4)?;
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

    let id = EntryId::from_str(&id_str).map_err(|e| {
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
    let entry_type = EntryType::from_str(&entry_type_str).map_err(|e| {
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

    Ok(Entry::restore(RestoreEntry {
        id,
        project,
        namespace,
        path,
        entry_type,
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
