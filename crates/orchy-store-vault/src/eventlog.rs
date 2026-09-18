use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use eventuary::fs::reader::{FsReader, FsReaderConfig};
use eventuary::fs::writer::{FsWriter, FsWriterConfig};
use eventuary::io::Writer;
use eventuary::{Event, Metadata, Namespace as EvNamespace, OrganizationId, StopAt};
use orchy_core::{
    ActorId, DomainError, DomainEvent, EventLog, EventQuery, MachineId, RecordedEvent, Result,
};

/// One log root per machine: `flock` cannot order offsets across a git remote, so two hosts
/// sharing a partition would corrupt silently.
pub struct EventuaryLog {
    writer: Arc<FsWriter>,
    root: PathBuf,
    organization: OrganizationId,
    actor: ActorId,
    machine: MachineId,
}

impl EventuaryLog {
    pub fn open(
        events_root: impl AsRef<Path>,
        organization: &str,
        actor: ActorId,
        machine: MachineId,
    ) -> Result<Self> {
        let root = events_root.as_ref().join(machine.to_string());
        std::fs::create_dir_all(&root)
            .map_err(|e| DomainError::validation(format!("creating event log root: {e}")))?;

        let writer = FsWriter::open(&root, FsWriterConfig::default())
            .map_err(|e| DomainError::validation(format!("opening event log: {e}")))?;

        Ok(Self {
            writer: Arc::new(writer),
            root,
            organization: OrganizationId::new(organization)
                .map_err(|e| DomainError::validation(format!("invalid organization: {e}")))?,
            actor,
            machine,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn to_eventuary(&self, event: &dyn DomainEvent) -> Result<Event> {
        let namespace = EvNamespace::new(event.namespace().as_str())
            .map_err(|e| DomainError::validation(format!("invalid event namespace: {e}")))?;

        Event::builder(
            self.organization.clone(),
            namespace,
            event.topic(),
            event.key().to_string(),
            event.payload()?,
        )
        .map_err(|e| DomainError::validation(format!("building event: {e}")))?
        .metadata(
            Metadata::new()
                .with("actor", self.actor.to_string())
                .and_then(|m| m.with("machine", self.machine.to_string()))
                .map_err(|e| DomainError::validation(format!("event metadata: {e}")))?,
        )
        .build()
        .map_err(|e| DomainError::validation(format!("building event: {e}")))
    }
}

#[async_trait]
impl EventLog for EventuaryLog {
    async fn append(&self, events: &[Box<dyn DomainEvent>]) -> Result<()> {
        for event in events {
            let wire = self.to_eventuary(event.as_ref())?;
            self.writer
                .write(&wire)
                .await
                .map_err(|e| DomainError::validation(format!("appending to event log: {e}")))?;
        }
        Ok(())
    }

    async fn replay(&self, query: &EventQuery) -> Result<Vec<RecordedEvent>> {
        let roots = machine_roots(self.root.parent().unwrap_or(&self.root))?;
        let mut all = Vec::new();

        for root in roots {
            let reader = FsReader::open(&root, FsReaderConfig::default())
                .map_err(|e| DomainError::validation(format!("opening event log: {e}")))?;
            for event in drain(&reader).await? {
                let recorded = from_eventuary(&event);
                if query.matches(&recorded) {
                    all.push(recorded);
                }
            }
        }

        // a shared total order is what makes projections deterministic without the writes
        // having had to be commutative
        all.sort_by(|a, b| {
            a.recorded_at
                .cmp(&b.recorded_at)
                .then_with(|| a.key.cmp(&b.key))
                .then_with(|| a.topic.cmp(&b.topic))
        });
        if let Some(limit) = query.limit {
            all.truncate(limit);
        }
        Ok(all)
    }
}

fn machine_roots(events_root: &Path) -> Result<Vec<PathBuf>> {
    let Ok(entries) = std::fs::read_dir(events_root) else {
        return Ok(Vec::new());
    };
    let mut roots: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.path())
        .collect();
    roots.sort();
    Ok(roots)
}

async fn drain(reader: &FsReader) -> Result<Vec<Event>> {
    use std::time::Duration;

    use eventuary::fs::reader::FsSubscription;
    use eventuary::io::Reader;
    use futures::StreamExt;
    use tokio::time::timeout;

    // CurrentEnd stops delivery but does not close the stream: a subscription is a tail by
    // design, so a bounded replay has to end on an idle gap
    const IDLE: Duration = Duration::from_millis(250);

    let subscription = FsSubscription {
        stop_at: StopAt::CurrentEnd,
        ..FsSubscription::earliest()
    };

    let stream = reader
        .read(subscription)
        .await
        .map_err(|e| DomainError::validation(format!("reading event log: {e}")))?;
    futures::pin_mut!(stream);

    let mut events = Vec::new();
    while let Ok(Some(message)) = timeout(IDLE, stream.next()).await {
        let message =
            message.map_err(|e| DomainError::validation(format!("reading event log: {e}")))?;
        let _ = message.ack().await;
        events.push(message.into_event());
    }
    Ok(events)
}

fn from_eventuary(event: &Event) -> RecordedEvent {
    RecordedEvent {
        topic: event.topic().as_str().to_owned(),
        key: event.key().to_string(),
        namespace: event.namespace().as_str().to_owned(),
        actor: event.metadata().get("actor").map(str::to_owned),
        machine: event.metadata().get("machine").map(str::to_owned),
        payload: serde_json::from_slice(event.payload().data()).unwrap_or(serde_json::Value::Null),
        recorded_at: recorded_at(event),
    }
}

fn recorded_at(event: &Event) -> DateTime<Utc> {
    event.timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchy_core::task::TaskCreated;
    use orchy_core::{Id, Namespace};

    const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    fn log(root: &Path) -> EventuaryLog {
        EventuaryLog::open(
            root,
            "orchy",
            ActorId::new("claude", MACHINE).unwrap(),
            MachineId::new(MACHINE).unwrap(),
        )
        .unwrap()
    }

    fn created(title: &str) -> Box<dyn DomainEvent> {
        Box::new(TaskCreated {
            id: Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap(),
            namespace: Namespace::new("/backend").unwrap(),
            title: title.to_owned(),
            parent: None,
            at: Utc::now(),
        })
    }

    #[tokio::test]
    async fn events_round_trip_through_a_real_on_disk_log() {
        let temp = tempfile::tempdir().unwrap();
        let log = log(temp.path());

        log.append(&[created("first"), created("second")])
            .await
            .unwrap();

        let replayed = log.replay(&EventQuery::default()).await.unwrap();
        assert_eq!(replayed.len(), 2);
        assert_eq!(replayed[0].topic, "task.created");
        assert_eq!(replayed[0].namespace, "/backend");
        assert_eq!(
            replayed[0].actor.as_deref(),
            Some("claude@01ARZ3NDEKTSV4RRFFQ69G5FAV")
        );
        assert_eq!(replayed[0].machine.as_deref(), Some(MACHINE));
        assert_eq!(replayed[0].payload["title"], "first");
    }

    #[tokio::test]
    async fn the_log_root_is_per_machine() {
        let temp = tempfile::tempdir().unwrap();
        let log = log(temp.path());
        assert!(
            log.root().ends_with(MACHINE),
            "each machine owns its own partition set (D33): {:?}",
            log.root()
        );
    }

    #[tokio::test]
    async fn replay_reads_every_machines_log_not_just_this_ones() {
        let temp = tempfile::tempdir().unwrap();
        let mine = log(temp.path());
        mine.append(&[created("mine")]).await.unwrap();

        let other_machine = "01BX5ZZKBKACTAV9WEVGEMMVRZ";
        let theirs = EventuaryLog::open(
            temp.path(),
            "orchy",
            ActorId::new("codex", other_machine).unwrap(),
            MachineId::new(other_machine).unwrap(),
        )
        .unwrap();
        theirs.append(&[created("theirs")]).await.unwrap();

        let replayed = mine.replay(&EventQuery::default()).await.unwrap();
        assert_eq!(
            replayed.len(),
            2,
            "a pulled vault must replay both machines' logs"
        );
    }

    #[tokio::test]
    async fn a_query_narrows_the_replay() {
        let temp = tempfile::tempdir().unwrap();
        let log = log(temp.path());
        log.append(&[created("a"), created("b"), created("c")])
            .await
            .unwrap();

        let limited = log
            .replay(&EventQuery {
                limit: Some(2),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(limited.len(), 2);

        let by_topic = log
            .replay(&EventQuery {
                topic_prefix: Some("document.".to_owned()),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(by_topic.is_empty(), "no document events were written");
    }

    #[tokio::test]
    async fn reopening_the_log_does_not_lose_earlier_events() {
        let temp = tempfile::tempdir().unwrap();
        log(temp.path()).append(&[created("first")]).await.unwrap();
        log(temp.path()).append(&[created("second")]).await.unwrap();

        let replayed = log(temp.path())
            .replay(&EventQuery::default())
            .await
            .unwrap();
        assert_eq!(replayed.len(), 2, "the log is append-only across processes");
    }
}
