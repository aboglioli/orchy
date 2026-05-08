use std::sync::Arc;

use orchy_store_conformance::{Backend, Bundle, conformance_suite};
use orchy_store_sqlite::{
    SqliteAgentStore, SqliteApiKeyStore, SqliteDatabase, SqliteEdgeStore, SqliteKnowledgeStore,
    SqliteLockStore, SqliteMessageStore, SqliteTaskStore,
};

struct SqliteBackend;

impl Backend for SqliteBackend {
    fn build() -> std::pin::Pin<Box<dyn std::future::Future<Output = Bundle> + Send>> {
        Box::pin(async {
            let db = SqliteDatabase::new(":memory:", None).unwrap();
            db.run_migrations(&SqliteDatabase::migrations_dir())
                .unwrap();
            let conn = db.conn();
            Bundle {
                agents: Arc::new(SqliteAgentStore::new(Arc::clone(&conn))),
                tasks: Arc::new(SqliteTaskStore::new(Arc::clone(&conn))),
                messages: Arc::new(SqliteMessageStore::new(Arc::clone(&conn))),
                knowledge: Arc::new(SqliteKnowledgeStore::new(Arc::clone(&conn))),
                edges: Arc::new(SqliteEdgeStore::new(Arc::clone(&conn))),
                locks: Arc::new(SqliteLockStore::new(Arc::clone(&conn))),
                api_keys: Arc::new(SqliteApiKeyStore::new(conn)),
            }
        })
    }
}

conformance_suite!(SqliteBackend);
