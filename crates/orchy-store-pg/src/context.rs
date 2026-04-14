use std::collections::HashMap;

use chrono::{DateTime, Utc};
use pgvector::Vector;
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{ContextSnapshot, ContextStore, RestoreContextSnapshot, SnapshotId};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{PgBackend, parse_pg_vector_text};

#[derive(Iden)]
enum Contexts {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "agent_id"]
    AgentId,
    #[iden = "namespace"]
    Namespace,
    #[iden = "summary"]
    Summary,
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "metadata"]
    Metadata,
    #[iden = "created_at"]
    CreatedAt,
}

impl ContextStore for PgBackend {
    async fn save(&self, snapshot: &ContextSnapshot) -> Result<()> {
        let vec_binding = snapshot.embedding().map(|e| Vector::from(e.to_vec()));
        let metadata_json = serde_json::to_value(snapshot.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO contexts (id, project, agent_id, namespace, summary, embedding, embedding_model, embedding_dimensions, metadata, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE
             SET summary = EXCLUDED.summary,
                 embedding = EXCLUDED.embedding,
                 embedding_model = EXCLUDED.embedding_model,
                 embedding_dimensions = EXCLUDED.embedding_dimensions,
                 metadata = EXCLUDED.metadata",
        )
        .bind(snapshot.id().as_uuid())
        .bind(snapshot.project().to_string())
        .bind(snapshot.agent_id().as_uuid())
        .bind(snapshot.namespace().to_string())
        .bind(snapshot.summary())
        .bind(vec_binding.as_ref())
        .bind(snapshot.embedding_model())
        .bind(snapshot.embedding_dimensions().map(|d| d as i32))
        .bind(&metadata_json)
        .bind(snapshot.created_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_latest(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        let row = sqlx::query(
            "SELECT id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at
             FROM contexts WHERE agent_id = $1
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(agent.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_context(&r)))
    }

    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        let mut select = Query::select();
        select
            .from(Contexts::Table)
            .expr(Expr::cust("id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at"));

        if !namespace.is_root() {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Contexts::Namespace).eq(namespace.to_string()))
                    .add(Expr::col(Contexts::Namespace).like(format!("{}/%", namespace))),
            );
        }

        if let Some(a) = agent {
            select.and_where(Expr::col(Contexts::AgentId).eq(*a.as_uuid()));
        }

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_context).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        let mut select = Query::select();
        select
            .from(Contexts::Table)
            .expr(Expr::cust("id, project, agent_id, namespace, summary, embedding::text, embedding_model, embedding_dimensions, metadata, created_at"))
            .and_where(Expr::cust_with_values(
                "to_tsvector('english', summary) @@ plainto_tsquery('english', ?)",
                [query.into()],
            ));

        if !namespace.is_root() {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Contexts::Namespace).eq(namespace.to_string()))
                    .add(Expr::col(Contexts::Namespace).like(format!("{}/%", namespace))),
            );
        }

        if let Some(a) = agent_id {
            select.and_where(Expr::col(Contexts::AgentId).eq(*a.as_uuid()));
        }

        select
            .order_by_expr(
                Expr::cust_with_values(
                    "ts_rank(to_tsvector('english', summary), plainto_tsquery('english', ?))",
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

        Ok(rows.iter().map(row_to_context).collect())
    }
}

fn row_to_context(row: &sqlx::postgres::PgRow) -> ContextSnapshot {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let agent_id: Uuid = row.get("agent_id");
    let namespace: String = row.get("namespace");
    let summary: String = row.get("summary");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let metadata: serde_json::Value = row.get("metadata");
    let created_at: DateTime<Utc> = row.get("created_at");

    ContextSnapshot::restore(RestoreContextSnapshot {
        id: SnapshotId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        agent_id: AgentId::from_uuid(agent_id),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        summary,
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        metadata: serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
        created_at,
    })
}
