use std::str::FromStr;

use chrono::{DateTime, Utc};
use pgvector::Vector;
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeStore, KnowledgeKind, RestoreKnowledge, Version,
};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{PgBackend, parse_pg_vector_text};

#[derive(Iden)]
enum KnowledgeEntries {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "path"]
    Path,
    #[iden = "kind"]
    KnowledgeKind,
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
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

const SELECT_COLUMNS: &str = "id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding::text, embedding_model, embedding_dimensions, created_at, updated_at";

impl KnowledgeStore for PgBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        let vec_binding = entry.embedding().map(|e| Vector::from(e.to_vec()));
        let tags_json = serde_json::to_value(entry.tags()).unwrap();
        let metadata_json = serde_json::to_value(entry.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO knowledge_entries (id, project, namespace, path, kind, title, content, tags, version, agent_id, metadata, embedding, embedding_model, embedding_dimensions, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
             ON CONFLICT (id) DO UPDATE SET
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                path = EXCLUDED.path,
                kind = EXCLUDED.kind,
                title = EXCLUDED.title,
                content = EXCLUDED.content,
                tags = EXCLUDED.tags,
                version = EXCLUDED.version,
                agent_id = EXCLUDED.agent_id,
                metadata = EXCLUDED.metadata,
                embedding = EXCLUDED.embedding,
                embedding_model = EXCLUDED.embedding_model,
                embedding_dimensions = EXCLUDED.embedding_dimensions,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(entry.id().as_uuid())
        .bind(entry.project().to_string())
        .bind(entry.namespace().to_string())
        .bind(entry.path())
        .bind(entry.kind().to_string())
        .bind(entry.title())
        .bind(entry.content())
        .bind(&tags_json)
        .bind(entry.version().as_u64() as i64)
        .bind(entry.agent_id().map(|a| *a.as_uuid()))
        .bind(&metadata_json)
        .bind(vec_binding.as_ref())
        .bind(entry.embedding_model())
        .bind(entry.embedding_dimensions().map(|d| d as i32))
        .bind(entry.created_at())
        .bind(entry.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = entry.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM knowledge_entries WHERE id = $1"
        ))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_entry(&r)))
    }

    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        let row = sqlx::query(&format!(
            "SELECT {SELECT_COLUMNS} FROM knowledge_entries WHERE project = $1 AND namespace = $2 AND path = $3"
        ))
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(path)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_entry(&r)))
    }

    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<Knowledge>> {
        let mut select = Query::select();
        select.from(KnowledgeEntries::Table).expr(Expr::cust(SELECT_COLUMNS));

        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(KnowledgeEntries::Project).eq(project.to_string()));
        }
        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                select.cond_where(
                    Cond::any()
                        .add(Expr::col(KnowledgeEntries::Namespace).eq(ns.to_string()))
                        .add(Expr::col(KnowledgeEntries::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref kind) = filter.kind {
            select.and_where(Expr::col(KnowledgeEntries::KnowledgeKind).eq(kind.to_string()));
        }
        if let Some(ref tag) = filter.tag {
            select.and_where(Expr::cust_with_values(
                "tags @> to_jsonb(?::text)",
                [tag.clone().into()],
            ));
        }
        if let Some(ref prefix) = filter.path_prefix {
            select.and_where(Expr::col(KnowledgeEntries::Path).like(format!("{prefix}%")));
        }
        if let Some(ref agent_id) = filter.agent_id {
            select.and_where(Expr::col(KnowledgeEntries::AgentId).eq(agent_id.to_string()));
        }

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_entry).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        let mut select = Query::select();
        select
            .from(KnowledgeEntries::Table)
            .expr(Expr::cust(SELECT_COLUMNS))
            .and_where(Expr::cust_with_values(
                "to_tsvector('english', title || ' ' || content) @@ plainto_tsquery('english', ?)",
                [query.into()],
            ));

        if let Some(ns) = namespace.filter(|ns| !ns.is_root()) {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(KnowledgeEntries::Namespace).eq(ns.to_string()))
                    .add(Expr::col(KnowledgeEntries::Namespace).like(format!("{}/%", ns))),
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

        Ok(rows.iter().map(row_to_entry).collect())
    }

    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        sqlx::query("DELETE FROM knowledge_entries WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_entry(row: &sqlx::postgres::PgRow) -> Knowledge {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let path: String = row.get("path");
    let kind_str: String = row.get("kind");
    let title: String = row.get("title");
    let content: String = row.get("content");
    let tags: serde_json::Value = row.get("tags");
    let version: i64 = row.get("version");
    let agent_id: Option<Uuid> = row.get("agent_id");
    let metadata: serde_json::Value = row.get("metadata");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    Knowledge::restore(RestoreKnowledge {
        id: KnowledgeId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        path,
        kind: KnowledgeKind::from_str(&kind_str).expect("invalid kind in database"),
        title,
        content,
        tags: serde_json::from_value(tags).unwrap_or_default(),
        version: Version::from(version as u64),
        agent_id: agent_id.map(AgentId::from_uuid),
        metadata: serde_json::from_value(metadata).unwrap_or_default(),
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        created_at,
        updated_at,
    })
}
