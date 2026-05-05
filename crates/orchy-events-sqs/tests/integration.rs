use std::time::Duration;

use aws_config::BehaviorVersion;
use aws_sdk_sqs::Client;
use aws_sdk_sqs::config::{Credentials, Region};
use futures::StreamExt;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use orchy_events::io::{Reader, Writer};
use orchy_events::{Event, OrganizationId, Payload};

use orchy_events_sqs::{SqsReader, SqsReaderConfig, SqsWriter};

async fn start_localstack() -> (ContainerAsync<GenericImage>, Client) {
    let container = GenericImage::new("localstack/localstack", "3")
        .with_exposed_port(4566.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Ready."))
        .with_env_var("SERVICES", "sqs")
        .start()
        .await
        .expect("start localstack");
    let port = container.get_host_port_ipv4(4566).await.unwrap();
    let endpoint = format!("http://127.0.0.1:{port}");
    let creds = Credentials::new("test", "test", None, None, "static");
    let cfg = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(endpoint)
        .credentials_provider(creds)
        .load()
        .await;
    let client = Client::new(&cfg);
    (container, client)
}

async fn create_queue(client: &Client, name: &str) -> String {
    client
        .create_queue()
        .queue_name(name)
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap()
}

fn make_event(org: &str, key: &str) -> Event {
    Event::create(org, "/x", "thing.happened", key, Payload::from_string("v")).unwrap()
}

#[tokio::test]
#[ignore = "requires localstack via testcontainers"]
async fn write_then_read_yields_event() {
    let (_c, client) = start_localstack().await;
    let queue_url = create_queue(&client, "q1").await;
    let writer = SqsWriter::new(client.clone(), &queue_url);
    let event = make_event("orgsqs", "k1");
    writer.write(&event).await.unwrap();

    let config =
        SqsReaderConfig::defaults_for(&queue_url, OrganizationId::new("orgsqs").unwrap()).unwrap();
    let reader = SqsReader::new(client.clone(), config);
    let mut stream = reader.read().await.unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg.event().key().as_str(), "k1");
    msg.ack().await.unwrap();
}

#[tokio::test]
#[ignore = "requires localstack via testcontainers"]
async fn write_all_chunks_to_batches_of_10() {
    let (_c, client) = start_localstack().await;
    let queue_url = create_queue(&client, "q-batch").await;
    let writer = SqsWriter::new(client.clone(), &queue_url);
    let events: Vec<Event> = (0..25)
        .map(|i| make_event("orgsqs", &format!("k{i}")))
        .collect();
    writer.write_all(&events).await.unwrap();

    let config =
        SqsReaderConfig::defaults_for(&queue_url, OrganizationId::new("orgsqs").unwrap()).unwrap();
    let reader = SqsReader::new(client.clone(), config);
    let mut stream = reader.read().await.unwrap();
    let mut received = 0usize;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while received < 25 && std::time::Instant::now() < deadline {
        if let Ok(Some(Ok(msg))) =
            tokio::time::timeout(Duration::from_secs(10), stream.next()).await
        {
            msg.ack().await.unwrap();
            received += 1;
        }
    }
    assert_eq!(received, 25);
}

#[tokio::test]
#[ignore = "requires localstack via testcontainers"]
async fn rejects_event_above_256kb() {
    let (_c, client) = start_localstack().await;
    let queue_url = create_queue(&client, "q-big").await;
    let writer = SqsWriter::new(client.clone(), &queue_url);
    let big = "x".repeat(300_000);
    let event = Event::create(
        "orgsqs",
        "/x",
        "thing.happened",
        "k",
        Payload::from_string(big),
    )
    .unwrap();
    assert!(writer.write(&event).await.is_err());
}

#[tokio::test]
#[ignore = "requires localstack via testcontainers"]
async fn org_mismatch_is_acked_and_skipped() {
    let (_c, client) = start_localstack().await;
    let queue_url = create_queue(&client, "q-org").await;
    let writer = SqsWriter::new(client.clone(), &queue_url);
    writer
        .write(&make_event("other-org", "wrong"))
        .await
        .unwrap();
    writer.write(&make_event("orgsqs", "right")).await.unwrap();

    let config =
        SqsReaderConfig::defaults_for(&queue_url, OrganizationId::new("orgsqs").unwrap()).unwrap();
    let reader = SqsReader::new(client.clone(), config);
    let mut stream = reader.read().await.unwrap();
    let msg = tokio::time::timeout(Duration::from_secs(30), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg.event().key().as_str(), "right");
}
