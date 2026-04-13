mod agent;
mod context;
mod memory;
mod message;
mod project;
mod skill;
mod store_impl;
mod task;

use sqlx::PgPool;

use orchy_core::error::{Error, Result};

pub struct PgBackend {
    pool: PgPool,
}

struct Migration {
    version: &'static str,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "20260412-160000",
        name: "initial_schema",
        sql: include_str!("../../../migrations/postgres/20260412-160000_initial_schema.sql"),
    },
    Migration {
        version: "20260412-235000",
        name: "notes_projects_reconnect",
        sql: include_str!(
            "../../../migrations/postgres/20260412-235000_notes_projects_reconnect.sql"
        ),
    },
];

impl PgBackend {
    pub async fn new(url: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Self::run_migrations(&pool).await?;
        Self::init_vector_indexes(&pool, embedding_dimensions).await?;

        Ok(Self { pool })
    }

    async fn run_migrations(pool: &PgPool) -> Result<()> {
        sqlx::query("SELECT pg_advisory_lock(42)")
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        for migration in MIGRATIONS {
            let applied: bool =
                sqlx::query_scalar("SELECT COUNT(*) > 0 FROM schema_migrations WHERE version = $1")
                    .bind(migration.version)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?;

            if !applied {
                sqlx::query(migration.sql)
                    .execute(pool)
                    .await
                    .map_err(|e| {
                        Error::Store(format!("migration {} failed: {e}", migration.name))
                    })?;

                sqlx::query("INSERT INTO schema_migrations (version, name) VALUES ($1, $2)")
                    .bind(migration.version)
                    .bind(migration.name)
                    .execute(pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?;
            }
        }

        sqlx::query("SELECT pg_advisory_unlock(42)")
            .execute(pool)
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
        sqlx::query("TRUNCATE contexts, messages, tasks, memory, skills, agents, projects CASCADE")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}
