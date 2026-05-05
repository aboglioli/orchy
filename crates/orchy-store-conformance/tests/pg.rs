#![cfg(feature = "integration-tests")]

use std::sync::Arc;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use orchy_store_conformance::Bundle;
use orchy_store_pg::{
    PgAgentStore, PgApiKeyStore, PgDatabase, PgEdgeStore, PgKnowledgeStore, PgLockStore,
    PgMessageStore, PgTaskStore,
};

async fn start_postgres() -> (ContainerAsync<GenericImage>, sqlx::PgPool) {
    let container = GenericImage::new("pgvector/pgvector", "0.8.2-pg17")
        .with_exposed_port(5432.tcp())
        .with_wait_for(WaitFor::message_on_stderr(
            "database system is ready to accept connections",
        ))
        .with_env_var("POSTGRES_USER", "orchy")
        .with_env_var("POSTGRES_PASSWORD", "orchy")
        .with_env_var("POSTGRES_DB", "orchy")
        .start()
        .await
        .expect("postgres start");
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://orchy:orchy@127.0.0.1:{port}/orchy");
    let db = PgDatabase::new(&url, None).await.unwrap();
    db.run_migrations(&PgDatabase::migrations_dir())
        .await
        .unwrap();
    (container, db.pool())
}

async fn bundle() -> (ContainerAsync<GenericImage>, Bundle) {
    let (container, p) = start_postgres().await;
    let bundle = Bundle {
        agents: Arc::new(PgAgentStore::new(p.clone())),
        tasks: Arc::new(PgTaskStore::new(p.clone())),
        messages: Arc::new(PgMessageStore::new(p.clone())),
        knowledge: Arc::new(PgKnowledgeStore::new(p.clone())),
        edges: Arc::new(PgEdgeStore::new(p.clone())),
        locks: Arc::new(PgLockStore::new(p.clone())),
        api_keys: Arc::new(PgApiKeyStore::new(p)),
    };
    (container, bundle)
}

#[tokio::test]
async fn task_save_then_find_returns_same() {
    let (_container, b) = bundle().await;
    orchy_store_conformance::scenarios::task_save_then_find_returns_same(&b).await;
}

#[tokio::test]
async fn task_filter_tag_does_not_match_substring() {
    let (_container, b) = bundle().await;
    orchy_store_conformance::scenarios::task_filter_tag_does_not_match_substring(&b).await;
}

#[tokio::test]
async fn knowledge_optimistic_concurrency() {
    let (_container, b) = bundle().await;
    orchy_store_conformance::scenarios::knowledge_optimistic_concurrency(&b).await;
}

#[tokio::test]
async fn message_claim_visibility() {
    let (_container, b) = bundle().await;
    orchy_store_conformance::scenarios::message_claim_visibility(&b).await;
}

#[tokio::test]
async fn edge_alias_blocks_normalizes_to_depends_on() {
    let (_container, b) = bundle().await;
    orchy_store_conformance::scenarios::edge_alias_blocks_normalizes_to_depends_on(&b).await;
}
