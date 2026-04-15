mod agent;
mod events;
mod knowledge;
mod message;
mod namespace;
mod organization;
mod project;
mod resource_lock;
mod review;
mod task;
mod watcher;

use std::path::Path;

use serde::de::DeserializeOwned;
use sqlx::PgPool;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};

pub struct PgBackend {
    pool: PgPool,
    embedding_dimensions: Option<u32>,
}

impl PgBackend {
    pub async fn new(url: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(Self {
            pool,
            embedding_dimensions,
        })
    }

    pub async fn run_migrations(&self, dir: &Path) -> Result<()> {
        sqlx::query("SELECT pg_advisory_lock(42)")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| Error::Store(format!("cannot read migrations dir: {e}")))?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "sql")
                    .unwrap_or(false)
            })
            .collect();
        files.sort_by_key(|e| e.file_name());

        for entry in files {
            let filename = entry.file_name().to_string_lossy().to_string();

            let applied: bool =
                sqlx::query_scalar("SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = $1")
                    .bind(&filename)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?;

            if applied {
                continue;
            }

            let sql = std::fs::read_to_string(entry.path())
                .map_err(|e| Error::Store(format!("cannot read {filename}: {e}")))?;

            sqlx::query(&sql)
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Store(format!("migration {filename} failed: {e}")))?;

            sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1)")
                .bind(&filename)
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;
        }

        self.init_vector_indexes().await?;

        sqlx::query("SELECT pg_advisory_unlock(42)")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn init_vector_indexes(&self) -> Result<()> {
        if self.embedding_dimensions.is_some() {
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS knowledge_entries_embedding_hnsw_idx
                 ON knowledge_entries USING hnsw (embedding vector_cosine_ops)",
            )
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn truncate_all(&self) -> Result<()> {
        sqlx::query(
            "TRUNCATE task_watchers, reviews, resource_locks, messages, tasks, knowledge_entries, events, namespaces, agents, projects CASCADE",
        )
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}

pub(crate) fn decode_json_value<T: DeserializeOwned>(
    value: serde_json::Value,
    table: &str,
    column: &str,
) -> Result<T> {
    serde_json::from_value(value)
        .map_err(|e| Error::Store(format!("invalid {table}.{column} JSON: {e}")))
}

pub(crate) fn parse_project_id(value: String, table: &str, column: &str) -> Result<ProjectId> {
    ProjectId::try_from(value.clone())
        .map_err(|e| Error::Store(format!("invalid {table}.{column} value `{value}`: {e}")))
}

pub(crate) fn parse_namespace(value: String, table: &str, column: &str) -> Result<Namespace> {
    Namespace::try_from(value.clone())
        .map_err(|e| Error::Store(format!("invalid {table}.{column} value `{value}`: {e}")))
}

pub(crate) fn parse_pg_vector_text(s: &str) -> Option<Vec<f32>> {
    let trimmed = s.trim_start_matches('[').trim_end_matches(']');
    if trimmed.is_empty() {
        return None;
    }
    let result: std::result::Result<Vec<f32>, _> = trimmed
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect();
    result.ok()
}
