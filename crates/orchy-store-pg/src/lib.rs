mod agent;
mod context;
mod document;
mod events;
mod memory;
mod message;
mod namespace;
mod project;
mod project_link;
mod resource_lock;
mod skill;
mod task;

use std::path::Path;

use sqlx::PgPool;

use orchy_core::error::{Error, Result};

pub struct PgBackend {
    pool: PgPool,
}

impl PgBackend {
    pub async fn new(url: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Self::init_vector_indexes(&pool, embedding_dimensions).await?;

        Ok(Self { pool })
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

        sqlx::query("SELECT pg_advisory_unlock(42)")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn init_vector_indexes(pool: &PgPool, embedding_dimensions: Option<u32>) -> Result<()> {
        if embedding_dimensions.is_some() {
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS memory_vec_idx ON memory USING hnsw (embedding vector_cosine_ops)",
            )
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

            sqlx::query(
                "CREATE INDEX IF NOT EXISTS contexts_vec_idx ON contexts USING hnsw (embedding vector_cosine_ops)",
            )
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn truncate_all(&self) -> Result<()> {
        sqlx::query("TRUNCATE contexts, messages, tasks, memory, skills, agents, projects, project_links CASCADE")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
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
