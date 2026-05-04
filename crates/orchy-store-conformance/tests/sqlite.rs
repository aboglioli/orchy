use std::sync::Arc;

use orchy_store_conformance::{Backend, Bundle, conformance_suite};
use orchy_store_sqlite::{
    SqliteAgentStore, SqliteApiKeyStore, SqliteDatabase, SqliteEdgeStore, SqliteKnowledgeStore,
    SqliteLockStore, SqliteMessageStore, SqliteTaskStore,
};

struct SqliteBackend;

#[async_trait::async_trait]
impl Backend for SqliteBackend {
    async fn build() -> Bundle {
        let db = SqliteDatabase::new(":memory:", None).unwrap();
        db.run_migrations(&SqliteDatabase::migrations_dir())
            .unwrap();
        let conn = db.conn();
        Bundle {
            agents: Arc::new(SqliteAgentStore::new(conn.clone())),
            tasks: Arc::new(SqliteTaskStore::new(conn.clone())),
            messages: Arc::new(SqliteMessageStore::new(conn.clone())),
            knowledge: Arc::new(SqliteKnowledgeStore::new(conn.clone())),
            edges: Arc::new(SqliteEdgeStore::new(conn.clone())),
            locks: Arc::new(SqliteLockStore::new(conn.clone())),
            api_keys: Arc::new(SqliteApiKeyStore::new(conn)),
        }
    }
}

conformance_suite!(SqliteBackend);
