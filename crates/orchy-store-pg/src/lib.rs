mod agent;
mod context;
mod memory;
mod message;
mod namespace;
mod project;
mod skill;
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

        Self::init_vector_indexes(&pool, embedding_dimensions).await?;

        Ok(Self { pool })
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
