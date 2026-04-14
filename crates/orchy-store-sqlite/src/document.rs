use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use sea_query::{Cond, Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use orchy_core::agent::AgentId;
use orchy_core::document::{Document, DocumentFilter, DocumentId, DocumentStore, RestoreDocument};
use orchy_core::error::{Error, Result};
use orchy_core::memory::Version;
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{SqliteBackend, bytes_to_embedding, embedding_to_bytes};

#[derive(Iden)]
enum Documents {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "path"]
    Path,
    #[iden = "title"]
    Title,
    #[iden = "content"]
    Content,
    #[iden = "tags"]
    Tags,
    #[iden = "version"]
    Version,
    #[iden = "embedding"]
    Embedding,
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "created_by"]
    CreatedBy,
    #[iden = "updated_by"]
    UpdatedBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

impl DocumentStore for SqliteBackend {
    async fn save(&self, doc: &Document) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let embedding_bytes = doc.embedding().map(embedding_to_bytes);
        let tags_json =
            serde_json::to_string(doc.tags()).map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "INSERT OR REPLACE INTO documents (id, project, namespace, path, title, content, tags, version, embedding, embedding_model, embedding_dimensions, created_by, updated_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                doc.id().to_string(),
                doc.project().to_string(),
                doc.namespace().to_string(),
                doc.path(),
                doc.title(),
                doc.content(),
                tags_json,
                doc.version().as_u64() as i64,
                embedding_bytes,
                doc.embedding_model(),
                doc.embedding_dimensions().map(|d| d as i64),
                doc.created_by().map(|a| a.to_string()),
                doc.updated_by().map(|a| a.to_string()),
                doc.created_at().to_rfc3339(),
                doc.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &DocumentId) -> Result<Option<Document>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, title, content, tags, version, embedding, embedding_model, embedding_dimensions, created_by, updated_by, created_at, updated_at
                 FROM documents WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_document)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Document>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, path, title, content, tags, version, embedding, embedding_model, embedding_dimensions, created_by, updated_by, created_at, updated_at
                 FROM documents WHERE project = ?1 AND namespace = ?2 AND path = ?3",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![project.to_string(), namespace.to_string(), path],
                row_to_document,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: DocumentFilter) -> Result<Vec<Document>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut query = Query::select();
        query.from(Documents::Table).columns([
            Documents::Id,
            Documents::Project,
            Documents::Namespace,
            Documents::Path,
            Documents::Title,
            Documents::Content,
            Documents::Tags,
            Documents::Version,
            Documents::Embedding,
            Documents::EmbeddingModel,
            Documents::EmbeddingDimensions,
            Documents::CreatedBy,
            Documents::UpdatedBy,
            Documents::CreatedAt,
            Documents::UpdatedAt,
        ]);

        if let Some(ref project) = filter.project {
            query.and_where(Expr::col(Documents::Project).eq(project.to_string()));
        }
        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                query.cond_where(
                    Cond::any()
                        .add(Expr::col(Documents::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Documents::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref tag) = filter.tag {
            query.and_where(Expr::col(Documents::Tags).like(format!("%\"{}\"%%", tag)));
        }
        if let Some(ref prefix) = filter.path_prefix {
            query.and_where(Expr::col(Documents::Path).like(format!("{}%", prefix)));
        }

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let documents = stmt
            .query_map(&*values.as_params(), row_to_document)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(documents)
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT d.id, d.project, d.namespace, d.path, d.title, d.content, d.tags, d.version, d.embedding, d.embedding_model, d.embedding_dimensions, d.created_by, d.updated_by, d.created_at, d.updated_at
             FROM documents d
             WHERE (d.title LIKE ?1 OR d.content LIKE ?1)",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let like_query = format!("%{}%", query);
        params.push(Box::new(like_query));
        let mut idx = 2;

        if let Some(ns) = namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (d.namespace = ?{idx} OR d.namespace LIKE ?{idx} || '/%')"
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

        let documents = stmt
            .query_map(param_refs.as_slice(), row_to_document)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(documents)
    }

    async fn delete(&self, id: &DocumentId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        conn.execute(
            "DELETE FROM documents WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<Document> {
    let id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let path: String = row.get(3)?;
    let title: String = row.get(4)?;
    let content: String = row.get(5)?;
    let tags_json: String = row.get(6)?;
    let version: i64 = row.get(7)?;
    let embedding_bytes: Option<Vec<u8>> = row.get(8)?;
    let embedding_model: Option<String> = row.get(9)?;
    let embedding_dimensions: Option<i64> = row.get(10)?;
    let created_by_str: Option<String> = row.get(11)?;
    let updated_by_str: Option<String> = row.get(12)?;
    let created_at_str: String = row.get(13)?;
    let updated_at_str: String = row.get(14)?;

    let id = DocumentId::from_str(&id_str).map_err(|e| {
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
    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(13, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Document::restore(RestoreDocument {
        id,
        project,
        namespace,
        path,
        title,
        content,
        tags,
        version: Version::from(version as u64),
        embedding: embedding_bytes.map(|b| bytes_to_embedding(&b)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        created_by: created_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        updated_by: updated_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at,
        updated_at,
    }))
}
