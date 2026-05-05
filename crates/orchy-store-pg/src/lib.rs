mod agent;
mod api_key;
mod edge;
mod events;
mod knowledge;
mod message;
mod namespace;
mod organization;
mod project;
mod reader;
mod reader_factory;
mod resource_lock;
mod task;
mod user;

pub use agent::PgAgentStore;
pub use api_key::PgApiKeyStore;
pub use edge::PgEdgeStore;
pub use events::PgEventWriter;
pub use knowledge::PgKnowledgeStore;
pub use message::PgMessageStore;
pub use namespace::PgNamespaceStore;
pub use organization::PgOrganizationStore;
pub use project::PgProjectStore;
pub use reader::{PgAcker, PgAckerVariant, PgReader, PgReaderConfig, PgStream};
pub use reader_factory::PgReaderFactory;
pub use resource_lock::PgLockStore;
pub use task::PgTaskStore;
pub use user::{PgOrgMembershipStore, PgUserStore};

use serde::de::DeserializeOwned;
use sqlx::PgPool;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::result::Result as StdResult;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};

pub struct PgDatabase {
    pool: PgPool,
    embedding_dimensions: Option<u32>,
}

impl PgDatabase {
    pub async fn new(url: &str, embedding_dimensions: Option<u32>) -> Result<Self> {
        let pool = PgPool::connect(url)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(Self {
            pool,
            embedding_dimensions,
        })
    }

    pub fn pool(&self) -> PgPool {
        self.pool.clone()
    }

    /// Create a pool synchronously via connect_lazy to avoid sqlx 0.8 lifetime issues.
    pub fn connect_lazy(url: &str, max_connections: u32) -> Result<PgPool> {
        use sqlx::postgres::PgPoolOptions;
        PgPoolOptions::new()
            .max_connections(max_connections)
            .connect_lazy(url)
            .map_err(|e| Error::Store(e.to_string()))
    }

    /// Run pending migrations on an arbitrary pool (no PgDatabase instance needed).
    pub async fn run_migrations_on_pool(pool: &PgPool, dir: &Path) -> Result<()> {
        let _embedding_dimensions: Option<u32> = None;
        Self::run_migrations_inner_static(pool, dir, None).await
    }

    pub fn embedding_dimensions(&self) -> Option<u32> {
        self.embedding_dimensions
    }

    pub async fn run_migrations(&self, dir: &Path) -> Result<()> {
        Self::run_migrations_inner_static(&self.pool, dir, self.embedding_dimensions).await
    }

    async fn run_migrations_inner_static(
        pool: &PgPool,
        dir: &Path,
        embedding_dimensions: Option<u32>,
    ) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version TEXT PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut files: Vec<_> = fs::read_dir(dir)
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
                    .fetch_one(pool)
                    .await
                    .map_err(|e| Error::Store(e.to_string()))?;

            if applied {
                continue;
            }

            let sql = fs::read_to_string(entry.path())
                .map_err(|e| Error::Store(format!("cannot read {filename}: {e}")))?;

            let mut tx = pool
                .begin()
                .await
                .map_err(|e| Error::Store(format!("migration {filename} tx begin: {e}")))?;
            sqlx::raw_sql(&sql)
                .execute(&mut *tx)
                .await
                .map_err(|e| Error::Store(format!("migration {filename} failed: {e}")))?;
            sqlx::query("INSERT INTO schema_migrations (version) VALUES ($1)")
                .bind(&filename)
                .execute(&mut *tx)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;
            tx.commit()
                .await
                .map_err(|e| Error::Store(format!("migration {filename} commit: {e}")))?;
        }

        Self::init_vector_indexes_static(pool, embedding_dimensions).await?;

        Ok(())
    }

    async fn init_vector_indexes_static(
        pool: &PgPool,
        embedding_dimensions: Option<u32>,
    ) -> Result<()> {
        let vector_count: bool =
            sqlx::query_scalar("SELECT COUNT(*) > 0 FROM pg_extension WHERE extname = 'vector'")
                .fetch_one(pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;

        if embedding_dimensions.is_some() && vector_count {
            sqlx::query(
                "CREATE INDEX IF NOT EXISTS knowledge_entries_embedding_hnsw_idx
                 ON knowledge_entries USING hnsw (embedding vector_cosine_ops)",
            )
            .execute(pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS tasks_assigned_roles_gin_idx ON tasks USING gin (assigned_roles jsonb_path_ops)",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS knowledge_entries_search_idx ON knowledge_entries USING gin (to_tsvector('english', title || ' ' || content))",
        )
        .execute(pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    pub fn migrations_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres")
    }

    pub async fn truncate_all(&self) -> Result<()> {
        sqlx::query(
            "TRUNCATE message_receipts, resource_locks, messages, tasks, knowledge_entries, \
             edges, events, namespaces, agents, projects, org_memberships, api_keys, \
             users, organizations, consumer_offsets CASCADE",
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
    let result: StdResult<Vec<f32>, _> = trimmed
        .split(',')
        .map(|v| v.trim().parse::<f32>())
        .collect();
    result.ok()
}
