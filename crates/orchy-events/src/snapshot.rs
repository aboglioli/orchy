use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::Result;
use crate::event::EventId;
use crate::payload::Payload;

pub trait Snapshot: Sized {
    fn take_snapshot(&self) -> Result<Payload>;
    fn from_snapshot(payload: &Payload) -> Result<Self>;
    fn snapshot_event_id(&self) -> EventId;
}

pub trait SnapshotEventId {
    fn snapshot_event_id(&self) -> EventId;
}

impl<T: Serialize + DeserializeOwned + SnapshotEventId> Snapshot for T {
    fn take_snapshot(&self) -> Result<Payload> {
        Payload::from_json(self)
    }

    fn from_snapshot(payload: &Payload) -> Result<Self> {
        payload.to_json()
    }

    fn snapshot_event_id(&self) -> EventId {
        SnapshotEventId::snapshot_event_id(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestAggregate {
        id: String,
        counter: u64,
        last_event_id: String,
    }

    impl SnapshotEventId for TestAggregate {
        fn snapshot_event_id(&self) -> EventId {
            self.last_event_id.parse().unwrap()
        }
    }

    #[test]
    fn snapshot_roundtrip() {
        let agg = TestAggregate {
            id: "a1".into(),
            counter: 42,
            last_event_id: "01900000-0000-7000-8000-000000000001".into(),
        };
        let payload = agg.take_snapshot().unwrap();
        let restored = TestAggregate::from_snapshot(&payload).unwrap();
        assert_eq!(restored, agg);
    }

    #[test]
    fn snapshot_event_id_returns_parsed_uuid() {
        let agg = TestAggregate {
            id: "a1".into(),
            counter: 1,
            last_event_id: "01900000-0000-7000-8000-000000000001".into(),
        };
        let id = Snapshot::snapshot_event_id(&agg);
        assert_eq!(id.to_string(), "01900000-0000-7000-8000-000000000001");
    }
}
