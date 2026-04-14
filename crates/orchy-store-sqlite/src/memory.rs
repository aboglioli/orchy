use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use sea_query::{Cond, Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore, RestoreMemoryEntry, Version};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

#[derive(Iden)]
enum Memory {
    Table,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "key"]
    Key,
    #[iden = "value"]
    Value,
    #[iden = "version"]
    Version,
    #[iden = "embedding"]
    Embedding,
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "locked"]
    Locked,
    #[iden = "written_by"]
    WrittenBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

impl MemoryStore for SqliteBackend {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let embedding_bytes = entry.embedding().map(embedding_to_bytes);

        conn.execute(
            "INSERT OR REPLACE INTO memory (project, namespace, key, value, version, embedding, embedding_model, embedding_dimensions, locked, written_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                entry.project().to_string(),
                entry.namespace().to_string(),
                entry.key(),
                entry.value(),
                entry.version().as_u64() as i64,
                embedding_bytes,
                entry.embedding_model(),
                entry.embedding_dimensions().map(|d| d as i64),
                entry.is_locked() as i64,
                entry.written_by().map(|a| a.to_string()),
                entry.created_at().to_rfc3339(),
                entry.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let rowid = conn.last_insert_rowid();

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

    async fn find_by_key(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<Option<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT project, namespace, key, value, version, embedding, embedding_model, embedding_dimensions, locked, written_by, created_at, updated_at
                 FROM memory WHERE project = ?1 AND namespace = ?2 AND key = ?3",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![project.to_string(), namespace.to_string(), key],
                row_to_memory,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut query = Query::select();
        query.from(Memory::Table).columns([
            Memory::Project,
            Memory::Namespace,
            Memory::Key,
            Memory::Value,
            Memory::Version,
            Memory::Embedding,
            Memory::EmbeddingModel,
            Memory::EmbeddingDimensions,
            Memory::Locked,
            Memory::WrittenBy,
            Memory::CreatedAt,
            Memory::UpdatedAt,
        ]);

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                query.cond_where(
                    Cond::any()
                        .add(Expr::col(Memory::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Memory::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref project) = filter.project {
            query.and_where(Expr::col(Memory::Project).eq(project.to_string()));
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let entries = stmt
            .query_map(&*values.as_params(), row_to_memory)
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
            "SELECT m.project, m.namespace, m.key, m.value, m.version, m.embedding, m.embedding_model, m.embedding_dimensions, m.locked, m.written_by, m.created_at, m.updated_at
             FROM memory m
             JOIN memory_fts ON memory_fts.rowid = m.rowid
             WHERE memory_fts MATCH ?1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let fts_query = sanitize_fts_query(query);
        params.push(Box::new(fts_query));
        let mut idx = 2;

        if let Some(ns) = namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (m.namespace = ?{idx} OR m.namespace LIKE ?{idx} || '/%')"
                ));
                params.push(Box::new(ns.to_string()));
                idx += 1;
            }
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

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, key: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let rowid: Option<i64> = conn
            .query_row(
                "SELECT rowid FROM memory WHERE project = ?1 AND namespace = ?2 AND key = ?3",
                rusqlite::params![project.to_string(), namespace.to_string(), key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        if let Some(rowid) = rowid {
            let _ = conn.execute(
                "INSERT INTO memory_fts(memory_fts, rowid, namespace, key, value) VALUES('delete', ?1, ?2, ?3, (SELECT value FROM memory WHERE project = ?4 AND namespace = ?2 AND key = ?3))",
                rusqlite::params![rowid, namespace.to_string(), key, project.to_string()],
            );

            let _ = conn.execute(
                "DELETE FROM memory_vec WHERE rowid = ?1",
                rusqlite::params![rowid],
            );
        }

        conn.execute(
            "DELETE FROM memory WHERE project = ?1 AND namespace = ?2 AND key = ?3",
            rusqlite::params![project.to_string(), namespace.to_string(), key],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryEntry> {
    let project_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let key: String = row.get(2)?;
    let value: String = row.get(3)?;
    let version: i64 = row.get(4)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(5)?;
    let embedding_model: Option<String> = row.get(6)?;
    let embedding_dimensions: Option<i64> = row.get(7)?;
    let locked: i64 = row.get(8)?;
    let written_by_str: Option<String> = row.get(9)?;
    let created_at_str: String = row.get(10)?;
    let updated_at_str: String = row.get(11)?;

    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(MemoryEntry::restore(RestoreMemoryEntry {
        project,
        namespace,
        key,
        value,
        version: Version::from(version as u64),
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        locked: locked != 0,
        written_by: written_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at,
        updated_at,
    }))
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
