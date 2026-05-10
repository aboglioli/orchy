use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;

use orchy_events::io::{Reader, Writer};
use orchy_events::{ConsumerGroupId, ConsumerStream, Event, OrganizationId, Payload, StartFrom};

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
            stream: ConsumerStream::default(),
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
        keys.push(msg.event().key().as_str().to_owned());
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
            stream: ConsumerStream::default(),
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

#[tokio::test]
async fn same_group_same_stream_resumes_from_offset() {
    let db = fresh_db();
    let writer = SqliteEventWriter::new(db.conn());
    for i in 0..3 {
        let e = Event::create(
            "orgrs",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let make = |limit: usize| {
        SqliteReader::new(
            db.conn(),
            SqliteReaderConfig {
                organization: OrganizationId::new("orgrs").unwrap(),
                consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
                stream: ConsumerStream::default(),
                start_from: StartFrom::Earliest,
                topics: None,
                namespace_prefix: None,
                end_at: Some(Utc::now()),
                limit: Some(limit),
                poll_interval: Duration::from_millis(50),
            },
        )
    };

    let mut s1 = make(2).read().await.unwrap();
    let mut first = Vec::new();
    while let Some(msg) = s1.next().await {
        let m = msg.unwrap();
        m.ack().await.unwrap();
        first.push(m.event().key().as_str().to_owned());
    }
    assert_eq!(first, vec!["k0", "k1"]);
    drop(s1);

    let mut s2 = make(5).read().await.unwrap();
    let mut second = Vec::new();
    while let Some(msg) = s2.next().await {
        let m = msg.unwrap();
        m.ack().await.unwrap();
        second.push(m.event().key().as_str().to_owned());
    }
    assert_eq!(second, vec!["k2"]);
}

#[tokio::test]
async fn same_group_different_stream_does_not_collide() {
    let db = fresh_db();
    let writer = SqliteEventWriter::new(db.conn());
    for i in 0..3 {
        let e = Event::create(
            "orgss",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let mut inbox = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("orgss").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::new("inbox").unwrap(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
            poll_interval: Duration::from_millis(50),
        },
    )
    .read()
    .await
    .unwrap();
    while let Some(msg) = inbox.next().await {
        msg.unwrap().ack().await.unwrap();
    }
    drop(inbox);

    let mut audit = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("orgss").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::new("audit").unwrap(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
            poll_interval: Duration::from_millis(50),
        },
    )
    .read()
    .await
    .unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = audit.next().await {
        keys.push(msg.unwrap().event().key().as_str().to_owned());
    }
    assert_eq!(keys, vec!["k0", "k1", "k2"]);
}

#[tokio::test]
async fn same_group_different_organization_does_not_collide() {
    let db = fresh_db();
    let writer = SqliteEventWriter::new(db.conn());
    for i in 0..3 {
        let e = Event::create(
            "org-x",
            "/x",
            "thing.happened",
            format!("kx{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }
    for i in 0..2 {
        let e = Event::create(
            "org-y",
            "/x",
            "thing.happened",
            format!("ky{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let mut x = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("org-x").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
            poll_interval: Duration::from_millis(50),
        },
    )
    .read()
    .await
    .unwrap();
    while let Some(msg) = x.next().await {
        msg.unwrap().ack().await.unwrap();
    }
    drop(x);

    let mut y = SqliteReader::new(
        db.conn(),
        SqliteReaderConfig {
            organization: OrganizationId::new("org-y").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(2),
            poll_interval: Duration::from_millis(50),
        },
    )
    .read()
    .await
    .unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = y.next().await {
        keys.push(msg.unwrap().event().key().as_str().to_owned());
    }
    assert_eq!(keys, vec!["ky0", "ky1"]);
}
