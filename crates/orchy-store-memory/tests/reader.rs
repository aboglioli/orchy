use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::StreamExt;
use tokio::sync::RwLock;

use orchy_events::io::{Reader, Writer};
use orchy_events::{
    ConsumerGroupId, ConsumerStream, Event, Namespace, OrganizationId, Payload, StartFrom, Topic,
};

use orchy_store_memory::{MemoryEventWriter, MemoryReader, MemoryReaderConfig, MemoryState};

#[tokio::test]
async fn streaming_reads_events_in_order() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    for i in 0..3 {
        let e = Event::create(
            "orga",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let reader = MemoryReader::new(
        state,
        offsets,
        MemoryReaderConfig {
            organization: OrganizationId::new("orga").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: Some(3),
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
async fn bounded_terminates_with_limit() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    for i in 0..5 {
        let e = Event::create(
            "orgb",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let reader = MemoryReader::new(
        state,
        offsets,
        MemoryReaderConfig {
            organization: OrganizationId::new("orgb").unwrap(),
            consumer_group_id: None,
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
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
async fn write_wakes_streaming_reader() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    let reader = MemoryReader::new(
        Arc::clone(&state),
        offsets,
        MemoryReaderConfig {
            organization: OrganizationId::new("orgc").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Latest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: None,
        },
    );

    let mut stream = reader.read().await.unwrap();

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let e = Event::create(
            "orgc",
            "/x",
            "thing.happened",
            "kx",
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    });

    let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(msg.event().key().as_str(), "kx");
}

#[tokio::test]
async fn topic_and_namespace_filters() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    let pairs = [
        ("a.b", "/backend/auth"),
        ("a.b", "/frontend"),
        ("c.d", "/backend"),
    ];
    for (i, (topic, ns)) in pairs.iter().enumerate() {
        let e = Event::create(
            "orgd",
            *ns,
            *topic,
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let reader = MemoryReader::new(
        state,
        offsets,
        MemoryReaderConfig {
            organization: OrganizationId::new("orgd").unwrap(),
            consumer_group_id: None,
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: Some(vec![Topic::new("a.b").unwrap()]),
            namespace_prefix: Some(Namespace::new("/backend").unwrap()),
            end_at: Some(Utc::now()),
            limit: Some(10),
        },
    );

    let mut stream = reader.read().await.unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = stream.next().await {
        keys.push(msg.unwrap().event().key().as_str().to_owned());
    }
    assert_eq!(keys, vec!["k0"]);
}

#[tokio::test]
async fn same_group_same_stream_resumes_from_offset() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    for i in 0..3 {
        let e = Event::create(
            "orgr",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let make_reader = || {
        MemoryReader::new(
            Arc::clone(&state),
            Arc::clone(&offsets),
            MemoryReaderConfig {
                organization: OrganizationId::new("orgr").unwrap(),
                consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
                stream: ConsumerStream::default(),
                start_from: StartFrom::Earliest,
                topics: None,
                namespace_prefix: None,
                end_at: Some(Utc::now()),
                limit: Some(2),
            },
        )
    };

    let mut s1 = make_reader().read().await.unwrap();
    let mut first_pass = Vec::new();
    while let Some(msg) = s1.next().await {
        let m = msg.unwrap();
        m.ack().await.unwrap();
        first_pass.push(m.event().key().as_str().to_owned());
    }
    assert_eq!(first_pass, vec!["k0", "k1"]);
    drop(s1);

    let mut s2 = make_reader().read().await.unwrap();
    let mut second_pass = Vec::new();
    while let Some(msg) = s2.next().await {
        let m = msg.unwrap();
        m.ack().await.unwrap();
        second_pass.push(m.event().key().as_str().to_owned());
    }
    assert_eq!(second_pass, vec!["k2"]);
}

#[tokio::test]
async fn same_group_different_stream_does_not_collide() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

    for i in 0..3 {
        let e = Event::create(
            "orgs",
            "/x",
            "thing.happened",
            format!("k{i}"),
            Payload::from_string("v"),
        )
        .unwrap();
        writer.write(&e).await.unwrap();
    }

    let mut consume_all = MemoryReader::new(
        Arc::clone(&state),
        Arc::clone(&offsets),
        MemoryReaderConfig {
            organization: OrganizationId::new("orgs").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::new("inbox").unwrap(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
        },
    )
    .read()
    .await
    .unwrap();
    while let Some(msg) = consume_all.next().await {
        msg.unwrap().ack().await.unwrap();
    }
    drop(consume_all);

    let mut other_stream = MemoryReader::new(
        Arc::clone(&state),
        Arc::clone(&offsets),
        MemoryReaderConfig {
            organization: OrganizationId::new("orgs").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::new("audit").unwrap(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
        },
    )
    .read()
    .await
    .unwrap();

    let mut keys = Vec::new();
    while let Some(msg) = other_stream.next().await {
        keys.push(msg.unwrap().event().key().as_str().to_owned());
    }
    assert_eq!(keys, vec!["k0", "k1", "k2"]);
}

#[tokio::test]
async fn same_group_different_organization_does_not_collide() {
    let state = Arc::new(MemoryState::new());
    let writer = MemoryEventWriter::new(Arc::clone(&state));
    let offsets = Arc::new(RwLock::new(Default::default()));

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

    let mut x = MemoryReader::new(
        Arc::clone(&state),
        Arc::clone(&offsets),
        MemoryReaderConfig {
            organization: OrganizationId::new("org-x").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(3),
        },
    )
    .read()
    .await
    .unwrap();
    while let Some(msg) = x.next().await {
        msg.unwrap().ack().await.unwrap();
    }
    drop(x);

    let mut y = MemoryReader::new(
        Arc::clone(&state),
        Arc::clone(&offsets),
        MemoryReaderConfig {
            organization: OrganizationId::new("org-y").unwrap(),
            consumer_group_id: Some(ConsumerGroupId::new("g").unwrap()),
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: Some(Utc::now()),
            limit: Some(2),
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
