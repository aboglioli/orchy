#![cfg(feature = "pg-tests")]

use std::sync::Arc;

use orchy_store_conformance::Bundle;
use orchy_store_pg::{
    PgAgentStore, PgApiKeyStore, PgDatabase, PgEdgeStore, PgKnowledgeStore, PgLockStore,
    PgMessageStore, PgTaskStore,
};

/// Serialize PG test access — all tests share the same database.
static PG_MUTEX: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn bundle() -> Bundle {
    let _guard = PG_MUTEX.lock().await;
    let db = PgDatabase::new("postgres://orchy:orchy@localhost:5432/orchy", None)
        .await
        .expect("failed to connect to postgres");
    db.run_migrations(&PgDatabase::migrations_dir())
        .await
        .expect("failed to run migrations");
    db.truncate_all().await.unwrap();
    let p = db.pool();
    Bundle {
        agents: Arc::new(PgAgentStore::new(p.clone())),
        tasks: Arc::new(PgTaskStore::new(p.clone())),
        messages: Arc::new(PgMessageStore::new(p.clone())),
        knowledge: Arc::new(PgKnowledgeStore::new(p.clone())),
        edges: Arc::new(PgEdgeStore::new(p.clone())),
        locks: Arc::new(PgLockStore::new(p.clone())),
        api_keys: Arc::new(PgApiKeyStore::new(p)),
    }
}

#[tokio::test]
async fn task_save_then_find_returns_same() {
    let b = bundle().await;
    orchy_store_conformance::scenarios::task_save_then_find_returns_same(&b).await;
}

#[tokio::test]
async fn task_filter_tag_does_not_match_substring() {
    let b = bundle().await;
    orchy_store_conformance::scenarios::task_filter_tag_does_not_match_substring(&b).await;
}

#[tokio::test]
async fn knowledge_optimistic_concurrency() {
    let b = bundle().await;
    orchy_store_conformance::scenarios::knowledge_optimistic_concurrency(&b).await;
}

#[tokio::test]
async fn message_claim_visibility() {
    let b = bundle().await;
    orchy_store_conformance::scenarios::message_claim_visibility(&b).await;
}

#[tokio::test]
async fn edge_alias_blocks_normalizes_to_depends_on() {
    let b = bundle().await;
    orchy_store_conformance::scenarios::edge_alias_blocks_normalizes_to_depends_on(&b).await;
}
