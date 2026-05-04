#![cfg(feature = "pg-tests")]

use std::sync::Arc;

use orchy_store_conformance::{Backend, Bundle, conformance_suite};
use orchy_store_pg::{
    PgAgentStore, PgApiKeyStore, PgDatabase, PgEdgeStore, PgKnowledgeStore, PgLockStore,
    PgMessageStore, PgTaskStore,
};

struct PgBackend;

#[async_trait::async_trait]
impl Backend for PgBackend {
    async fn build() -> Bundle {
        let url = std::env::var("DATABASE_URL")
            .expect("DATABASE_URL must be set to run pg conformance tests");
        let db = PgDatabase::new(&url, None)
            .await
            .expect("failed to connect to postgres");
        db.run_migrations(&orchy_store_pg::PgDatabase::migrations_dir())
            .await
            .expect("failed to run migrations");
        let pool = db.pool();
        Bundle {
            agents: Arc::new(PgAgentStore::new(pool.clone())),
            tasks: Arc::new(PgTaskStore::new(pool.clone())),
            messages: Arc::new(PgMessageStore::new(pool.clone())),
            knowledge: Arc::new(PgKnowledgeStore::new(pool.clone())),
            edges: Arc::new(PgEdgeStore::new(pool.clone())),
            locks: Arc::new(PgLockStore::new(pool.clone())),
            api_keys: Arc::new(PgApiKeyStore::new(pool)),
        }
    }
}

conformance_suite!(PgBackend);
