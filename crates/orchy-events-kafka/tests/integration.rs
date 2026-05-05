use std::net::TcpListener;
use std::time::Duration;

use futures::StreamExt;
use rdkafka::ClientConfig;
use rdkafka::admin::{AdminClient, AdminOptions, NewTopic, TopicReplication};
use rdkafka::client::DefaultClientContext;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};

use orchy_events::Payload;
use orchy_events::io::{Reader, Writer};
use orchy_events::{ConsumerGroupId, Event, OrganizationId, StartFrom};

use orchy_events_kafka::{KafkaReader, KafkaReaderConfig, KafkaWriter, PartitionAssignment};

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn start_kafka() -> (ContainerAsync<GenericImage>, String) {
    let host_port = free_port();
    let advertised = format!("PLAINTEXT://localhost:{host_port}");
    let container = GenericImage::new("confluentinc/cp-kafka", "7.6.0")
        .with_exposed_port(9092.tcp())
        .with_wait_for(WaitFor::message_on_stdout("Kafka Server started"))
        .with_mapped_port(host_port, 9092.tcp())
        .with_env_var("CLUSTER_ID", "MkU3OEVBNTcwNTJENDM2Qk")
        .with_env_var("KAFKA_NODE_ID", "1")
        .with_env_var("KAFKA_PROCESS_ROLES", "broker,controller")
        .with_env_var(
            "KAFKA_LISTENERS",
            "PLAINTEXT://0.0.0.0:9092,CONTROLLER://0.0.0.0:9093",
        )
        .with_env_var("KAFKA_ADVERTISED_LISTENERS", advertised)
        .with_env_var("KAFKA_CONTROLLER_LISTENER_NAMES", "CONTROLLER")
        .with_env_var(
            "KAFKA_LISTENER_SECURITY_PROTOCOL_MAP",
            "CONTROLLER:PLAINTEXT,PLAINTEXT:PLAINTEXT",
        )
        .with_env_var("KAFKA_CONTROLLER_QUORUM_VOTERS", "1@localhost:9093")
        .with_env_var("KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR", "1")
        .start()
        .await
        .expect("kafka start");
    let brokers = format!("localhost:{host_port}");
    (container, brokers)
}

async fn create_topic(brokers: &str, topic: &str, partitions: i32) {
    let admin: AdminClient<DefaultClientContext> = ClientConfig::new()
        .set("bootstrap.servers", brokers)
        .create()
        .unwrap();
    admin
        .create_topics(
            &[NewTopic::new(topic, partitions, TopicReplication::Fixed(1))],
            &AdminOptions::new(),
        )
        .await
        .unwrap();
}

fn make_event(org: &str, topic: &str, key: &str) -> Event {
    Event::create(org, "/x", topic, key, Payload::from_string("v")).unwrap()
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn write_then_read_single_partition() {
    let (_c, brokers) = start_kafka().await;
    create_topic(&brokers, "topic-a", 1).await;
    let writer = KafkaWriter::new(std::slice::from_ref(&brokers), "topic-a").unwrap();
    writer
        .write(&make_event("orgk", "thing.happened", "k1"))
        .await
        .unwrap();

    let cfg = KafkaReaderConfig {
        start_from: StartFrom::Earliest,
        ..KafkaReaderConfig::streaming(
            vec![brokers.clone()],
            vec!["topic-a".to_string()],
            ConsumerGroupId::new("g1").unwrap(),
            OrganizationId::new("orgk").unwrap(),
        )
    };
    let reader = KafkaReader::new(cfg).unwrap();
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
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn three_partitions_all_received() {
    let (_c, brokers) = start_kafka().await;
    create_topic(&brokers, "topic-3", 3).await;
    let writer = KafkaWriter::new(std::slice::from_ref(&brokers), "topic-3").unwrap();
    for i in 0..30 {
        writer
            .write(&make_event("orgk", "thing.happened", &format!("k{i}")))
            .await
            .unwrap();
    }

    let cfg = KafkaReaderConfig {
        start_from: StartFrom::Earliest,
        ..KafkaReaderConfig::streaming(
            vec![brokers.clone()],
            vec!["topic-3".to_string()],
            ConsumerGroupId::new("g3").unwrap(),
            OrganizationId::new("orgk").unwrap(),
        )
    };
    let reader = KafkaReader::new(cfg).unwrap();
    let mut stream = reader.read().await.unwrap();
    let mut count = 0usize;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while count < 30 && std::time::Instant::now() < deadline {
        if let Ok(Some(Ok(msg))) =
            tokio::time::timeout(Duration::from_secs(10), stream.next()).await
        {
            msg.ack().await.unwrap();
            count += 1;
        }
    }
    assert_eq!(count, 30);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn specific_partition_assignment() {
    let (_c, brokers) = start_kafka().await;
    create_topic(&brokers, "topic-pa", 3).await;
    let writer = KafkaWriter::new(std::slice::from_ref(&brokers), "topic-pa").unwrap();
    for i in 0..15 {
        writer
            .write(&make_event("orgk", "thing.happened", &format!("k{i}")))
            .await
            .unwrap();
    }

    let cfg = KafkaReaderConfig {
        start_from: StartFrom::Earliest,
        partition_assignment: PartitionAssignment::Specific(vec![0]),
        ..KafkaReaderConfig::streaming(
            vec![brokers.clone()],
            vec!["topic-pa".to_string()],
            ConsumerGroupId::new("g-pa").unwrap(),
            OrganizationId::new("orgk").unwrap(),
        )
    };
    let reader = KafkaReader::new(cfg).unwrap();
    let mut stream = reader.read().await.unwrap();
    let mut count = 0usize;
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < deadline {
        if let Ok(Some(Ok(msg))) = tokio::time::timeout(Duration::from_secs(5), stream.next()).await
        {
            msg.ack().await.unwrap();
            count += 1;
        } else {
            break;
        }
    }
    assert!(count > 0 && count < 15);
}

#[tokio::test]
#[cfg_attr(not(feature = "integration-tests"), ignore)]
async fn limit_terminates_stream() {
    let (_c, brokers) = start_kafka().await;
    create_topic(&brokers, "topic-lim", 1).await;
    let writer = KafkaWriter::new(std::slice::from_ref(&brokers), "topic-lim").unwrap();
    for i in 0..10 {
        writer
            .write(&make_event("orgk", "thing.happened", &format!("k{i}")))
            .await
            .unwrap();
    }

    let cfg = KafkaReaderConfig {
        start_from: StartFrom::Earliest,
        limit: Some(3),
        ..KafkaReaderConfig::streaming(
            vec![brokers.clone()],
            vec!["topic-lim".to_string()],
            ConsumerGroupId::new("g-lim").unwrap(),
            OrganizationId::new("orgk").unwrap(),
        )
    };
    let reader = KafkaReader::new(cfg).unwrap();
    let mut stream = reader.read().await.unwrap();
    let mut count = 0usize;
    while let Some(Ok(msg)) = tokio::time::timeout(Duration::from_secs(15), stream.next())
        .await
        .ok()
        .flatten()
    {
        msg.ack().await.unwrap();
        count += 1;
    }
    assert_eq!(count, 3);
}
