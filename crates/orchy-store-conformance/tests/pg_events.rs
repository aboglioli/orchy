#![cfg(feature = "integration-tests")]

use std::sync::Arc;
use std::time::Duration;

use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use orchy_events::io::{ArcWriter, BoxReader, ReaderExt, WriterExt};

use orchy_store_conformance::events::{
    EventBackendCapabilities, EventHarness, ReaderFactory, ReaderSpec,
};
use orchy_store_pg::{PgDatabase, PgEventWriter, PgReader, PgReaderConfig};

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

async fn harness() -> (ContainerAsync<GenericImage>, EventHarness) {
    let (container, pool) = start_postgres().await;

    let writer: ArcWriter = PgEventWriter::new(pool.clone()).into_arced();

    let pool_for_factory = pool.clone();
    let reader_factory: ReaderFactory = Arc::new(move |spec: ReaderSpec| {
        let pool = pool_for_factory.clone();
        Box::pin(async move {
            let reader = PgReader::new(
                pool,
                PgReaderConfig {
                    organization: spec.organization,
                    consumer_group_id: spec.consumer_group_id,
                    stream: spec.stream,
                    start_from: spec.start_from,
                    topics: spec.topics,
                    namespace_prefix: spec.namespace_prefix,
                    end_at: spec.end_at,
                    limit: spec.limit,
                    batch_size: 100,
                    poll_interval: Duration::from_millis(100),
                },
            );
            let boxed: BoxReader = reader.into_boxed();
            boxed
        })
    });

    (
        container,
        EventHarness {
            writer,
            reader_factory,
        },
    )
}

fn caps() -> EventBackendCapabilities {
    EventBackendCapabilities {
        supports_replay: true,
        supports_timestamp_start: true,
        supports_nack_redelivery: false,
        preserves_total_order: true,
    }
}

#[tokio::test]
async fn write_then_read_roundtrip() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::write_then_read_roundtrip(&h, caps()).await;
}

#[tokio::test]
async fn write_all_preserves_all_events() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::write_all_preserves_all_events(&h, caps()).await;
}

#[tokio::test]
async fn ordering_preserved() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::ordering_preserved(&h, caps()).await;
}

#[tokio::test]
async fn start_from_earliest() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::start_from_earliest(&h, caps()).await;
}

#[tokio::test]
async fn start_from_latest() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::start_from_latest(&h, caps()).await;
}

#[tokio::test]
async fn start_from_timestamp() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::start_from_timestamp(&h, caps()).await;
}

#[tokio::test]
async fn topic_filter_matches() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::topic_filter_matches(&h, caps()).await;
}

#[tokio::test]
async fn namespace_prefix_filter() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::namespace_prefix_filter(&h, caps()).await;
}

#[tokio::test]
async fn ack_advances_checkpoint() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::ack_advances_checkpoint(&h, caps()).await;
}

#[tokio::test]
async fn independent_consumer_groups() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::independent_consumer_groups(&h, caps()).await;
}

#[tokio::test]
async fn independent_streams() {
    let (_c, h) = harness().await;
    orchy_store_conformance::events::independent_streams(&h, caps()).await;
}
