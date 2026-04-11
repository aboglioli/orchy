mod agent;
mod context;
mod memory;
mod message;
mod store_impl;
mod task;

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

        Self::init_schema(&pool, embedding_dimensions).await?;

        Ok(Self { pool })
    }

    async fn init_schema(pool: &PgPool, embedding_dimensions: Option<u32>) -> Result<()> {
        // Use advisory lock to prevent race conditions during parallel schema init
        sqlx::query("SELECT pg_advisory_lock(42)")
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS agents (
                id UUID PRIMARY KEY,
                namespace TEXT,
                roles JSONB NOT NULL DEFAULT '[]',
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'online',
                last_heartbeat TIMESTAMPTZ NOT NULL,
                connected_at TIMESTAMPTZ NOT NULL,
                metadata JSONB NOT NULL DEFAULT '{}'
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS tasks (
                id UUID PRIMARY KEY,
                namespace TEXT NOT NULL,
                title TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                priority TEXT NOT NULL DEFAULT 'normal',
                assigned_roles JSONB NOT NULL DEFAULT '[]',
                claimed_by UUID REFERENCES agents(id),
                claimed_at TIMESTAMPTZ,
                depends_on JSONB NOT NULL DEFAULT '[]',
                result_summary TEXT,
                created_by UUID REFERENCES agents(id),
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS memory (
                namespace TEXT NOT NULL,
                key TEXT NOT NULL,
                value TEXT NOT NULL,
                version BIGINT NOT NULL DEFAULT 1,
                embedding VECTOR,
                embedding_model TEXT,
                embedding_dimensions INTEGER,
                written_by UUID REFERENCES agents(id),
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                PRIMARY KEY (namespace, key)
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS memory_fts_idx ON memory USING gin(to_tsvector('english', value))",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS messages (
                id UUID PRIMARY KEY,
                namespace TEXT,
                from_agent UUID NOT NULL REFERENCES agents(id),
                to_target TEXT NOT NULL,
                body TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS contexts (
                id UUID PRIMARY KEY,
                agent_id UUID NOT NULL REFERENCES agents(id),
                namespace TEXT,
                summary TEXT NOT NULL,
                embedding VECTOR,
                embedding_model TEXT,
                embedding_dimensions INTEGER,
                metadata JSONB NOT NULL DEFAULT '{}',
                created_at TIMESTAMPTZ NOT NULL
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS contexts_fts_idx ON contexts USING gin(to_tsvector('english', summary))",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

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

        // Release advisory lock
        sqlx::query("SELECT pg_advisory_unlock(42)")
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    /// Truncate all tables (useful for tests).
    pub async fn truncate_all(&self) -> Result<()> {
        sqlx::query("TRUNCATE contexts, messages, tasks, memory, agents CASCADE")
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}
