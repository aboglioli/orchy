use chrono::{DateTime, Utc};
use pgvector::Vector;
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::document::{Document, DocumentFilter, DocumentId, DocumentStore, RestoreDocument};
use orchy_core::error::{Error, Result};
use orchy_core::memory::Version;
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{PgBackend, parse_pg_vector_text};

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

const SELECT_COLUMNS: &str = "id, project, namespace, path, title, content, tags, version, embedding::text, embedding_model, embedding_dimensions, created_by, updated_by, created_at, updated_at";

impl DocumentStore for PgBackend {
    async fn save(&self, doc: &mut Document) -> Result<()> {
        let vec_binding = doc.embedding().map(|e| Vector::from(e.to_vec()));
        let tags_json = serde_json::to_value(doc.tags()).unwrap();

        sqlx::query(
            "INSERT INTO documents (id, project, namespace, path, title, content, tags, version, embedding, embedding_model, embedding_dimensions, created_by, updated_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
             ON CONFLICT (id) DO UPDATE SET
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                path = EXCLUDED.path,
                title = EXCLUDED.title,
                content = EXCLUDED.content,
                tags = EXCLUDED.tags,
                version = EXCLUDED.version,
                embedding = EXCLUDED.embedding,
                embedding_model = EXCLUDED.embedding_model,
                embedding_dimensions = EXCLUDED.embedding_dimensions,
                updated_by = EXCLUDED.updated_by,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(doc.id().as_uuid())
        .bind(doc.project().to_string())
        .bind(doc.namespace().to_string())
        .bind(doc.path())
        .bind(doc.title())
        .bind(doc.content())
        .bind(&tags_json)
        .bind(doc.version().as_u64() as i64)
        .bind(vec_binding.as_ref())
        .bind(doc.embedding_model())
        .bind(doc.embedding_dimensions().map(|d| d as i32))
        .bind(doc.created_by().map(|a| *a.as_uuid()))
        .bind(doc.updated_by().map(|a| *a.as_uuid()))
        .bind(doc.created_at())
        .bind(doc.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = doc.drain_events();
        for evt in &events {
            if let Ok(serialized) = orchy_events::SerializedEvent::from_event(evt) {
                let id = uuid::Uuid::parse_str(&serialized.id).unwrap();
                let _ = sqlx::query(
                    "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
                )
                .bind(id)
                .bind(&serialized.organization)
                .bind(&serialized.namespace)
                .bind(&serialized.topic)
                .bind(&serialized.payload)
                .bind(&serialized.content_type)
                .bind(serde_json::to_value(&serialized.metadata).unwrap())
                .bind(serialized.timestamp)
                .bind(serialized.version as i64)
                .execute(&self.pool)
                .await;
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &DocumentId) -> Result<Option<Document>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM documents WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_document(&r)))
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Document>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM documents WHERE project = $1 AND namespace = $2 AND path = $3"
        ))
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_document(&r)))
    }

    async fn list(&self, filter: DocumentFilter) -> Result<Vec<Document>> {
        let mut select = Query::select();
        select
            .from(Documents::Table)
            .expr(Expr::cust(SELECT_COLUMNS));

        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(Documents::Project).eq(project.to_string()));
        }
        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                select.cond_where(
                    Cond::any()
                        .add(Expr::col(Documents::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Documents::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref tag) = filter.tag {
            select.and_where(Expr::cust_with_values(
                "tags @> to_jsonb(?::text)",
                [tag.clone().into()],
            ));
        }
        if let Some(ref prefix) = filter.path_prefix {
            select.and_where(Expr::col(Documents::Path).like(format!("{prefix}%")));
        }

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_document).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        let mut select = Query::select();
        select
            .from(Documents::Table)
            .expr(Expr::cust(SELECT_COLUMNS))
            .and_where(Expr::cust_with_values(
                "to_tsvector('english', title || ' ' || content) @@ plainto_tsquery('english', ?)",
                [query.into()],
            ));

        if let Some(ns) = namespace.filter(|ns| !ns.is_root()) {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Documents::Namespace).eq(ns.to_string()))
                    .add(Expr::col(Documents::Namespace).like(format!("{}/%", ns))),
            );
        }

        select
            .order_by_expr(
                Expr::cust_with_values(
                    "ts_rank(to_tsvector('english', title || ' ' || content), plainto_tsquery('english', ?))",
                    [query.into()],
                ),
                sea_query::Order::Desc,
            )
            .limit(limit as u64);

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_document).collect())
    }

    async fn delete(&self, id: &DocumentId) -> Result<()> {
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_document(row: &sqlx::postgres::PgRow) -> Document {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let path: String = row.get("path");
    let title: String = row.get("title");
    let content: String = row.get("content");
    let tags: serde_json::Value = row.get("tags");
    let version: i64 = row.get("version");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let created_by: Option<Uuid> = row.get("created_by");
    let updated_by: Option<Uuid> = row.get("updated_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    Document::restore(RestoreDocument {
        id: DocumentId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        path,
        title,
        content,
        tags: serde_json::from_value(tags).unwrap_or_default(),
        version: Version::from(version as u64),
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        created_by: created_by.map(AgentId::from_uuid),
        updated_by: updated_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    })
}
