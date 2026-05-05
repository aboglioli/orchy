use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;

use orchy_events::io::{Reader, Writer};
use orchy_events::{ConsumerGroupId, Event, OrganizationId, Payload, StartFrom};

use orchy_store_sqlite::{SqliteDatabase, SqliteEventWriter, SqliteReader, SqliteReaderConfig};

fn fresh_db() -> SqliteDatabase {
    let db = SqliteDatabase::new(":memory:", None).unwrap();
    db.run_migrations(&SqliteDatabase::migrations_dir())
        .unwrap();
    db
}

#[tokio::test]
async fn streaming_yields_then_blocks_for_more() {
    let db = fresh_db();
    let writer = SqliteEventWriter::new(db.conn());
    for i in 0..3 {
        let e = Event::create(
            "orgsl",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    let reader = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("orgsl").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: Some(3),
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader.read().await.unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = stream.next().await {
        let msg = msg.unwrap();
        msg.ack().await.unwrap();
        keys.push(msg.event().key().as_str().to_string());
    }
    assert_eq!(keys, vec!["k0", "k1", "k2"]);
}

#[tokio::test]
async fn bounded_terminates() {
    let db = fresh_db();
    let writer = SqliteEventWriter::new(db.conn());
    for i in 0..5 {
        let e = Event::create(
            "orgsl2",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    let reader = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("orgsl2").unwrap(),
            consumer_group_id: None,
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
            poll_interval: Duration::from_millis(50),
        },
    );
    let mut stream = reader.read().await.unwrap();
    let mut count = 0;
    while let Some(_msg) = stream.next().await {
        count += 1;
    }
    assert_eq!(count, 3);
}
