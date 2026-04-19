use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};
use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeKind, KnowledgeStore, RestoreKnowledge,
    Version,
};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};

#[async_trait]
impl KnowledgeStore for SqliteBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        let embedding_bytes = entry.embedding().map(embedding_to_bytes);
        let tags_json =
            serde_json::to_string(entry.tags()).map_err(|e| Error::Store(e.to_string()))?;
        let metadata_json =
            serde_json::to_string(entry.metadata()).map_err(|e| Error::Store(e.to_string()))?;

        let params = rusqlite::params![
            entry.id().to_string(),
            entry.org_id().to_string(),
            entry.project().map(|p| p.to_string()),
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
        ];

        if let Some(pv) = entry.persisted_version() {
            let rows = tx.execute(
                "UPDATE knowledge_entries SET organization_id = ?2, project = ?3, namespace = ?4, path = ?5, kind = ?6, title = ?7, content = ?8, tags = ?9, version = ?10, agent_id = ?11, metadata = ?12, embedding = ?13, embedding_model = ?14, embedding_dimensions = ?15, created_at = ?16, updated_at = ?17
                 WHERE id = ?1 AND version = ?18",
                rusqlite::params![
                    entry.id().to_string(),
                    entry.org_id().to_string(),
                    entry.project().map(|p| p.to_string()),
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
                    pv.as_u64() as i64,
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

            if rows == 0 {
                let stored_version: Option<i64> = tx
                    .query_row(
                        "SELECT version FROM knowledge_entries WHERE id = ?1",
                        rusqlite::params![entry.id().to_string()],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|e| Error::Store(e.to_string()))?;

                return Err(match stored_version {
                    Some(v) => Error::VersionMismatch {
                        expected: pv.as_u64(),
                        actual: v as u64,
                    },
                    None => Error::NotFound(format!("knowledge entry {}", entry.id())),
                });
            }
        } else {
            tx.execute(
                "INSERT INTO knowledge_entries (id, organization_id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
                params,
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = entry.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;

        entry.mark_persisted();

        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
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
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let result = if let Some(proj) = project {
            let mut stmt = conn
                .prepare(
                    "SELECT id, organization_id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                     FROM knowledge_entries WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND path = ?4",
                )
                .map_err(|e| Error::Store(e.to_string()))?;

            stmt.query_row(
                rusqlite::params![
                    org.to_string(),
                    proj.to_string(),
                    namespace.to_string(),
                    path
                ],
                row_to_entry,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, organization_id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at
                     FROM knowledge_entries WHERE organization_id = ?1 AND project IS NULL AND namespace = ?2 AND path = ?3",
                )
                .map_err(|e| Error::Store(e.to_string()))?;

            stmt.query_row(
                rusqlite::params![org.to_string(), namespace.to_string(), path],
                row_to_entry,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
        };

        Ok(result)
    }

    async fn list(&self, filter: KnowledgeFilter, page: PageParams) -> Result<Page<Knowledge>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, organization_id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at FROM knowledge_entries WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref org_id) = filter.org_id {
            sql.push_str(&format!(" AND organization_id = ?{idx}"));
            params.push(Box::new(org_id.to_string()));
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            if filter.include_org_level {
                sql.push_str(&format!(" AND (project = ?{idx} OR project IS NULL)"));
            } else {
                sql.push_str(&format!(" AND project = ?{idx}"));
            }
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

        if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                sql.push_str(&format!(" AND id < ?{idx}"));
                params.push(Box::new(decoded));
                idx += 1;
            }
        }

        let _ = idx;
        sql.push_str(" ORDER BY id DESC");
        let fetch_limit = (page.limit as u64).saturating_add(1);
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut entries: Vec<Knowledge> = stmt
            .query_map(param_refs.as_slice(), row_to_entry)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        let has_more = entries.len() > page.limit as usize;
        if has_more {
            entries.truncate(page.limit as usize);
        }
        let next_cursor = if has_more {
            entries.last().map(|e| encode_cursor(&e.id().to_string()))
        } else {
            None
        };

        Ok(Page::new(entries, next_cursor))
    }

    async fn search(
        &self,
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<(Knowledge, Option<f32>)>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        if let Some(emb) = embedding {
            let vec_ready = conn
                .query_row(
                    "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_vec' LIMIT 1",
                    [],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|e| Error::Store(e.to_string()))?
                .is_some();

            if vec_ready {
                return search_knowledge_vec(&conn, org, emb, namespace, limit)
                    .map(|v| v.into_iter().map(|k| (k, None)).collect());
            }
        }

        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let fts_ready = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_entries_fts' LIMIT 1",
                [],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
            .is_some();

        if fts_ready {
            search_knowledge_fts(&conn, org, query, namespace, limit)
                .map(|v| v.into_iter().map(|k| (k, None)).collect())
        } else {
            search_knowledge_like(&conn, org, query, namespace, limit)
                .map(|v| v.into_iter().map(|k| (k, None)).collect())
        }
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

fn search_knowledge_vec(
    conn: &rusqlite::Connection,
    org: &OrganizationId,
    embedding: &[f32],
    namespace: Option<&Namespace>,
    limit: usize,
) -> Result<Vec<Knowledge>> {
    let emb_bytes = embedding_to_bytes(embedding);

    let mut sql = String::from(
        "SELECT e.id, e.organization_id, e.project, e.namespace, e.path, e.kind, e.title, e.content, e.tags, e.version, e.agent_id, e.metadata, e.embedding, e.embedding_model, e.embedding_dimensions, e.created_at, e.updated_at
         FROM knowledge_vec kv
         JOIN knowledge_entries e ON e.rowid = kv.rowid
         WHERE kv.embedding MATCH ?1 AND kv.k = ?2 AND e.organization_id = ?3",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(emb_bytes));
    params.push(Box::new(limit as i64));
    params.push(Box::new(org.to_string()));
    let mut idx = 4;

    if let Some(ns) = namespace {
        if !ns.is_root() {
            sql.push_str(&format!(
                " AND (e.namespace = ?{idx} OR e.namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
    }

    let _ = idx;
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Error::Store(e.to_string()))?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let entries = stmt
        .query_map(param_refs.as_slice(), row_to_entry)
        .map_err(|e| Error::Store(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Store(e.to_string()))?;

    Ok(entries)
}

fn sanitize_fts5_query(q: &str) -> String {
    q.split_whitespace()
        .map(|w| format!("\"{}\"", w.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn search_knowledge_fts(
    conn: &rusqlite::Connection,
    org: &OrganizationId,
    query: &str,
    namespace: Option<&Namespace>,
    limit: usize,
) -> Result<Vec<Knowledge>> {
    let mut sql = String::from(
        "SELECT e.id, e.organization_id, e.project, e.namespace, e.path, e.kind, e.title, e.content, e.tags, e.version, e.agent_id, e.metadata, e.embedding, e.embedding_model, e.embedding_dimensions, e.created_at, e.updated_at
         FROM knowledge_entries_fts
         JOIN knowledge_entries AS e ON e.id = knowledge_entries_fts.knowledge_id
         WHERE knowledge_entries_fts MATCH ?1 AND e.organization_id = ?2",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(sanitize_fts5_query(query)));
    params.push(Box::new(org.to_string()));
    let mut idx = 3;

    if let Some(ns) = namespace {
        if !ns.is_root() {
            sql.push_str(&format!(
                " AND (e.namespace = ?{idx} OR e.namespace LIKE ?{idx} || '/%')"
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
    }

    sql.push_str(&format!(
        " ORDER BY bm25(knowledge_entries_fts) LIMIT ?{idx}"
    ));
    params.push(Box::new(limit as i64));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| Error::Store(e.to_string()))?;
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let entries = stmt
        .query_map(param_refs.as_slice(), row_to_entry)
        .map_err(|e| Error::Store(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Store(e.to_string()))?;

    Ok(entries)
}

fn search_knowledge_like(
    conn: &rusqlite::Connection,
    org: &OrganizationId,
    query: &str,
    namespace: Option<&Namespace>,
    limit: usize,
) -> Result<Vec<Knowledge>> {
    let mut sql = String::from(
        "SELECT e.id, e.organization_id, e.project, e.namespace, e.path, e.kind, e.title, e.content, e.tags, e.version, e.agent_id, e.metadata, e.embedding, e.embedding_model, e.embedding_dimensions, e.created_at, e.updated_at
         FROM knowledge_entries e
         WHERE e.organization_id = ?1 AND (e.title LIKE ?2 OR e.content LIKE ?2 OR e.path LIKE ?2)",
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    params.push(Box::new(org.to_string()));
    let like_query = format!("%{query}%");
    params.push(Box::new(like_query));
    let mut idx = 3;

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
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    let entries = stmt
        .query_map(param_refs.as_slice(), row_to_entry)
        .map_err(|e| Error::Store(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Store(e.to_string()))?;

    Ok(entries)
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<Knowledge> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let project_str: Option<String> = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let path: String = row.get(4)?;
    let kind_str: String = row.get(5)?;
    let title: String = row.get(6)?;
    let content: String = row.get(7)?;
    let tags_json: String = row.get(8)?;
    let version: i64 = row.get(9)?;
    let agent_id_str: Option<String> = row.get(10)?;
    let metadata_json: String = row.get(11)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(12)?;
    let embedding_model: Option<String> = row.get(13)?;
    let embedding_dimensions: Option<i64> = row.get(14)?;
    let created_at_str: String = row.get(15)?;
    let updated_at_str: String = row.get(16)?;

    let id = KnowledgeId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;
    let project = project_str
        .map(ProjectId::try_from)
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                2,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let kind = KnowledgeKind::from_str(&kind_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let metadata: HashMap<String, String> = serde_json::from_str(&metadata_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(15, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Knowledge::restore(RestoreKnowledge {
        id,
        org_id,
        project,
        namespace,
        path,
        kind,
        title,
        content,
        tags,
        version: Version::new(version as u64),
        agent_id: agent_id_str.and_then(|s| AgentId::from_str(&s).ok()),
        metadata,
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        created_at,
        updated_at,
    }))
}
