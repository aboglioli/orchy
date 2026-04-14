use chrono::{DateTime, Utc};
use pgvector::Vector;
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::memory::{MemoryEntry, MemoryFilter, MemoryStore, RestoreMemoryEntry, Version};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::{PgBackend, parse_pg_vector_text};

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
    #[iden = "embedding_model"]
    EmbeddingModel,
    #[iden = "embedding_dimensions"]
    EmbeddingDimensions,
    #[iden = "written_by"]
    WrittenBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

impl MemoryStore for PgBackend {
    async fn save(&self, entry: &mut MemoryEntry) -> Result<()> {
        let vec_binding = entry.embedding().map(|e| Vector::from(e.to_vec()));

        sqlx::query(
            "INSERT INTO memory (project, namespace, key, value, version, embedding, embedding_model, embedding_dimensions, written_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (project, namespace, key) DO UPDATE
             SET value = EXCLUDED.value,
                 version = EXCLUDED.version,
                 embedding = EXCLUDED.embedding,
                 embedding_model = EXCLUDED.embedding_model,
                 embedding_dimensions = EXCLUDED.embedding_dimensions,
                 written_by = EXCLUDED.written_by,
                 updated_at = EXCLUDED.updated_at",
        )
        .bind(entry.project().to_string())
        .bind(entry.namespace().to_string())
        .bind(entry.key())
        .bind(entry.value())
        .bind(entry.version().as_u64() as i64)
        .bind(vec_binding.as_ref())
        .bind(entry.embedding_model())
        .bind(entry.embedding_dimensions().map(|d| d as i32))
        .bind(entry.written_by().map(|a| *a.as_uuid()))
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

    async fn find_by_key(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<Option<MemoryEntry>> {
        let row = sqlx::query(
            "SELECT project, namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at
             FROM memory WHERE project = $1 AND namespace = $2 AND key = $3",
        )
        .bind(project.to_string())
        .bind(namespace.to_string())
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_memory(&r)))
    }

    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        let mut select = Query::select();
        select
            .from(Memory::Table)
            .expr(Expr::cust("project, namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at"));

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                select.cond_where(
                    Cond::any()
                        .add(Expr::col(Memory::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Memory::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(Memory::Project).eq(project.to_string()));
        }

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_memory).collect())
    }

    async fn search(
        &self,
        query: &str,
        _embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let mut select = Query::select();
        select
            .from(Memory::Table)
            .expr(Expr::cust("project, namespace, key, value, version, embedding::text, embedding_model, embedding_dimensions, written_by, created_at, updated_at"))
            .and_where(Expr::cust_with_values(
                "to_tsvector('english', value) @@ plainto_tsquery('english', ?)",
                [query.into()],
            ));

        if let Some(ns) = namespace.filter(|ns| !ns.is_root()) {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Memory::Namespace).eq(ns.to_string()))
                    .add(Expr::col(Memory::Namespace).like(format!("{}/%", ns))),
            );
        }

        select
            .order_by_expr(
                Expr::cust_with_values(
                    "ts_rank(to_tsvector('english', value), plainto_tsquery('english', ?))",
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

        Ok(rows.iter().map(row_to_memory).collect())
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, key: &str) -> Result<()> {
        sqlx::query("DELETE FROM memory WHERE project = $1 AND namespace = $2 AND key = $3")
            .bind(project.to_string())
            .bind(namespace.to_string())
            .bind(key)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }
}

fn row_to_memory(row: &sqlx::postgres::PgRow) -> MemoryEntry {
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let key: String = row.get("key");
    let value: String = row.get("value");
    let version: i64 = row.get("version");
    let embedding_str: Option<String> = row.get("embedding");
    let embedding_model: Option<String> = row.get("embedding_model");
    let embedding_dimensions: Option<i32> = row.get("embedding_dimensions");
    let written_by: Option<Uuid> = row.get("written_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    MemoryEntry::restore(RestoreMemoryEntry {
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        key,
        value,
        version: Version::from(version as u64),
        embedding: embedding_str.and_then(|s| parse_pg_vector_text(&s)),
        embedding_model,
        embedding_dimensions: embedding_dimensions.map(|d| d as u32),
        written_by: written_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    })
}
