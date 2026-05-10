use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use futures::StreamExt;

use orchy_events::io::{ArcWriter, BoxReader, Reader, Writer};
use orchy_events::{
    ConsumerGroupId, ConsumerStream, Event, Namespace, OrganizationId, Payload, StartFrom, Topic,
};

#[derive(Debug, Clone, Copy)]
pub struct EventBackendCapabilities {
    pub supports_replay: bool,
    pub supports_timestamp_start: bool,
    pub supports_nack_redelivery: bool,
    pub preserves_total_order: bool,
}

#[derive(Clone)]
pub struct ReaderSpec {
    pub organization: OrganizationId,
    pub consumer_group_id: Option<ConsumerGroupId>,
    pub stream: ConsumerStream,
    pub start_from: StartFrom,
    pub topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

impl ReaderSpec {
    pub fn new(organization: OrganizationId) -> Self {
        Self {
            organization,
            consumer_group_id: None,
            stream: ConsumerStream::default(),
            start_from: StartFrom::Earliest,
            topics: None,
            namespace_prefix: None,
            end_at: None,
            limit: None,
        }
    }
}

pub type ReaderFactory =
    Arc<dyn Fn(ReaderSpec) -> Pin<Box<dyn Future<Output = BoxReader> + Send>> + Send + Sync>;

pub struct EventHarness {
    pub writer: ArcWriter,
    pub reader_factory: ReaderFactory,
}

pub trait EventBackend: Send + Sync + 'static {
    fn capabilities() -> EventBackendCapabilities;
    fn build() -> Pin<Box<dyn Future<Output = EventHarness> + Send>>;
}

#[macro_export]
macro_rules! event_conformance_suite {
    ($backend:ty) => {
        $crate::declare_event_test! { $backend, write_then_read_roundtrip, $crate::events::write_then_read_roundtrip }
        $crate::declare_event_test! { $backend, write_all_preserves_all_events, $crate::events::write_all_preserves_all_events }
        $crate::declare_event_test! { $backend, ordering_preserved, $crate::events::ordering_preserved }
        $crate::declare_event_test! { $backend, start_from_earliest, $crate::events::start_from_earliest }
        $crate::declare_event_test! { $backend, start_from_latest, $crate::events::start_from_latest }
        $crate::declare_event_test! { $backend, start_from_timestamp, $crate::events::start_from_timestamp }
        $crate::declare_event_test! { $backend, topic_filter_matches, $crate::events::topic_filter_matches }
        $crate::declare_event_test! { $backend, namespace_prefix_filter, $crate::events::namespace_prefix_filter }
        $crate::declare_event_test! { $backend, ack_advances_checkpoint, $crate::events::ack_advances_checkpoint }
        $crate::declare_event_test! { $backend, independent_consumer_groups, $crate::events::independent_consumer_groups }
        $crate::declare_event_test! { $backend, independent_streams, $crate::events::independent_streams }
    };
}

#[macro_export]
macro_rules! declare_event_test {
    ($backend:ty, $name:ident, $scenario:path) => {
        #[tokio::test]
        async fn $name() {
            let harness = <$backend as $crate::events::EventBackend>::build().await;
            let caps = <$backend as $crate::events::EventBackend>::capabilities();
            $scenario(&harness, caps).await;
        }
    };
}

fn make_event(org: &str, namespace: &str, topic: &str, key: &str) -> Event {
    Event::create(org, namespace, topic, key, Payload::from_string("v")).unwrap()
}

async fn drain(reader: &BoxReader) -> Vec<String> {
    let mut stream = reader.read().await.unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = stream.next().await {
        let m = msg.unwrap();
        m.ack().await.unwrap();
        keys.push(m.event().key().as_str().to_owned());
    }
    keys
}

async fn drain_no_ack(reader: &BoxReader) -> Vec<String> {
    let mut stream = reader.read().await.unwrap();
    let mut keys = Vec::new();
    while let Some(msg) = stream.next().await {
        let m = msg.unwrap();
        keys.push(m.event().key().as_str().to_owned());
    }
    keys
}

pub async fn write_then_read_roundtrip(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-rt";
    let ev = make_event(org, "/x", "thing.happened", "k0");
    harness.writer.write(&ev).await.unwrap();

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.start_from = StartFrom::Earliest;
    spec.end_at = Some(Utc::now());
    spec.limit = Some(1);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert_eq!(keys, vec!["k0".to_owned()]);
}

pub async fn write_all_preserves_all_events(
    harness: &EventHarness,
    _caps: EventBackendCapabilities,
) {
    let org = "conf-wa";
    let events: Vec<Event> = (0..5)
        .map(|i| make_event(org, "/x", "thing.happened", &format!("k{i}")))
        .collect();
    harness.writer.write_all(&events).await.unwrap();

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.end_at = Some(Utc::now());
    spec.limit = Some(5);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert_eq!(keys.len(), 5);
}

pub async fn ordering_preserved(harness: &EventHarness, caps: EventBackendCapabilities) {
    if !caps.preserves_total_order {
        return;
    }
    let org = "conf-ord";
    for i in 0..4 {
        let e = make_event(org, "/x", "thing.happened", &format!("k{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.end_at = Some(Utc::now());
    spec.limit = Some(4);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert_eq!(keys, vec!["k0", "k1", "k2", "k3"]);
}

pub async fn start_from_earliest(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-earliest";
    for i in 0..3 {
        let e = make_event(org, "/x", "thing.happened", &format!("k{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.start_from = StartFrom::Earliest;
    spec.end_at = Some(Utc::now());
    spec.limit = Some(3);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert_eq!(keys.len(), 3);
}

pub async fn start_from_latest(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-latest";
    for i in 0..3 {
        let e = make_event(org, "/x", "thing.happened", &format!("old{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.start_from = StartFrom::Latest;
    spec.end_at = Some(Utc::now());
    spec.limit = Some(3);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert!(
        keys.is_empty(),
        "StartFrom::Latest must skip historical events, got {keys:?}"
    );
}

pub async fn start_from_timestamp(harness: &EventHarness, caps: EventBackendCapabilities) {
    if !caps.supports_timestamp_start {
        return;
    }
    let org = "conf-ts";
    let pre = make_event(org, "/x", "thing.happened", "before");
    harness.writer.write(&pre).await.unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    let cutoff = Utc::now();
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let post = make_event(org, "/x", "thing.happened", "after");
    harness.writer.write(&post).await.unwrap();

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.start_from = StartFrom::Timestamp(cutoff);
    spec.end_at = Some(Utc::now());
    spec.limit = Some(5);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert!(
        keys.contains(&"after".to_owned()) && !keys.contains(&"before".to_owned()),
        "timestamp filter wrong, got {keys:?}"
    );
}

pub async fn topic_filter_matches(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-topic";
    harness
        .writer
        .write(&make_event(org, "/x", "task.created", "k1"))
        .await
        .unwrap();
    harness
        .writer
        .write(&make_event(org, "/x", "task.deleted", "k2"))
        .await
        .unwrap();

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.topics = Some(vec![Topic::new("task.created").unwrap()]);
    spec.end_at = Some(Utc::now());
    spec.limit = Some(5);

    let reader = (harness.reader_factory)(spec).await;
    let keys = drain(&reader).await;
    assert_eq!(keys, vec!["k1".to_owned()]);
}

pub async fn namespace_prefix_filter(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-ns";
    harness
        .writer
        .write(&make_event(org, "/foo", "thing.happened", "kfoo"))
        .await
        .unwrap();
    harness
        .writer
        .write(&make_event(org, "/bar", "thing.happened", "kbar"))
        .await
        .unwrap();
    harness
        .writer
        .write(&make_event(org, "/foo/sub", "thing.happened", "ksub"))
        .await
        .unwrap();

    let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
    spec.namespace_prefix = Some(Namespace::new("/foo").unwrap());
    spec.end_at = Some(Utc::now());
    spec.limit = Some(5);

    let reader = (harness.reader_factory)(spec).await;
    let mut keys = drain(&reader).await;
    keys.sort();
    assert_eq!(keys, vec!["kfoo".to_owned(), "ksub".to_owned()]);
}

pub async fn ack_advances_checkpoint(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-ack";
    for i in 0..3 {
        let e = make_event(org, "/x", "thing.happened", &format!("k{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let make = |limit: usize| {
        let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
        spec.consumer_group_id = Some(ConsumerGroupId::new("g").unwrap());
        spec.stream = ConsumerStream::default();
        spec.start_from = StartFrom::Earliest;
        spec.end_at = Some(Utc::now());
        spec.limit = Some(limit);
        spec
    };

    let r1 = (harness.reader_factory)(make(2)).await;
    let first = drain(&r1).await;
    assert_eq!(first, vec!["k0".to_owned(), "k1".to_owned()]);
    drop(r1);

    let r2 = (harness.reader_factory)(make(5)).await;
    let second = drain(&r2).await;
    assert_eq!(second, vec!["k2".to_owned()]);
}

pub async fn independent_consumer_groups(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-grp";
    for i in 0..3 {
        let e = make_event(org, "/x", "thing.happened", &format!("k{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let make = |group: &str| {
        let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
        spec.consumer_group_id = Some(ConsumerGroupId::new(group).unwrap());
        spec.start_from = StartFrom::Earliest;
        spec.end_at = Some(Utc::now());
        spec.limit = Some(3);
        spec
    };

    let ra = (harness.reader_factory)(make("group-a")).await;
    let a_keys = drain(&ra).await;
    drop(ra);

    let rb = (harness.reader_factory)(make("group-b")).await;
    let b_keys = drain(&rb).await;

    assert_eq!(a_keys.len(), 3);
    assert_eq!(b_keys.len(), 3);
    assert_eq!(a_keys, b_keys);
}

pub async fn independent_streams(harness: &EventHarness, _caps: EventBackendCapabilities) {
    let org = "conf-stream";
    for i in 0..3 {
        let e = make_event(org, "/x", "thing.happened", &format!("k{i}"));
        harness.writer.write(&e).await.unwrap();
    }

    let make = |stream: &str| {
        let mut spec = ReaderSpec::new(OrganizationId::new(org).unwrap());
        spec.consumer_group_id = Some(ConsumerGroupId::new("g").unwrap());
        spec.stream = ConsumerStream::new(stream).unwrap();
        spec.start_from = StartFrom::Earliest;
        spec.end_at = Some(Utc::now());
        spec.limit = Some(3);
        spec
    };

    let inbox = (harness.reader_factory)(make("inbox")).await;
    let inbox_keys = drain(&inbox).await;
    drop(inbox);

    let audit = (harness.reader_factory)(make("audit")).await;
    let audit_keys = drain_no_ack(&audit).await;

    assert_eq!(inbox_keys.len(), 3);
    assert_eq!(audit_keys.len(), 3);
    assert_eq!(inbox_keys, audit_keys);
}
