use std::fmt::Debug;

use eventuary::{Payload, Topic};
use serde::Serialize;

use crate::error::{DomainError, Result};
use crate::id::Id;
use crate::namespace::Namespace;

pub trait DomainEvent: Debug + Send + Sync {
    fn topic(&self) -> Topic;
    fn key(&self) -> Id;
    fn namespace(&self) -> Namespace;
    fn payload(&self) -> Result<Payload>;
}

pub fn topic(value: &'static str) -> Topic {
    Topic::new(value).expect("domain topics are compile-time constants and must be valid")
}

pub fn payload_of<T: Serialize>(value: &T) -> Result<Payload> {
    Payload::from_json(value)
        .map_err(|e| DomainError::validation(format!("event payload is not serializable: {e}")))
}

#[derive(Debug, Default)]
pub struct EventCollector(Vec<Box<dyn DomainEvent>>);

impl EventCollector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn collect(&mut self, event: impl DomainEvent + 'static) {
        self.0.push(Box::new(event));
    }

    pub fn drain(&mut self) -> Vec<Box<dyn DomainEvent>> {
        std::mem::take(&mut self.0)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Clone for EventCollector {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl PartialEq for EventCollector {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for EventCollector {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, serde::Deserialize)]
pub struct RecordedEvent {
    pub topic: String,
    pub key: String,
    pub namespace: String,
    pub actor: Option<String>,
    pub machine: Option<String>,
    pub payload: serde_json::Value,
    pub recorded_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Default)]
pub struct EventQuery {
    pub topic_prefix: Option<String>,
    pub key: Option<Id>,
    pub actor: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<usize>,
}

impl EventQuery {
    pub fn keep_latest(&self, events: &mut Vec<RecordedEvent>) {
        if let Some(limit) = self.limit
            && events.len() > limit
        {
            events.drain(..events.len() - limit);
        }
    }

    pub fn matches(&self, event: &RecordedEvent) -> bool {
        if let Some(prefix) = &self.topic_prefix
            && !event.topic.starts_with(prefix)
        {
            return false;
        }
        if let Some(key) = &self.key
            && event.key != key.to_string()
        {
            return false;
        }
        if let Some(actor) = &self.actor
            && !event.actor.as_deref().is_some_and(|by| is_by(by, actor))
        {
            return false;
        }
        if let Some(since) = self.since
            && event.recorded_at < since
        {
            return false;
        }
        true
    }
}

#[async_trait::async_trait]
pub trait EventLog: Send + Sync {
    async fn append(&self, events: &[Box<dyn DomainEvent>]) -> Result<()>;
    async fn replay(&self, query: &EventQuery) -> Result<Vec<RecordedEvent>>;
}

/// `alias` names that alias on every machine; `alias@machine` names one actor.
fn is_by(recorded: &str, wanted: &str) -> bool {
    if wanted.contains('@') {
        return recorded == wanted;
    }
    recorded.split('@').next() == Some(wanted)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorded(key: &str) -> RecordedEvent {
        RecordedEvent {
            topic: "task.created".to_owned(),
            key: key.to_owned(),
            namespace: "/".to_owned(),
            actor: None,
            machine: None,
            payload: serde_json::Value::Null,
            recorded_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn a_limit_keeps_the_most_recent_events_in_order() {
        let mut events: Vec<RecordedEvent> = ["1", "2", "3", "4", "5"].map(recorded).to_vec();
        EventQuery {
            limit: Some(2),
            ..Default::default()
        }
        .keep_latest(&mut events);
        let keys: Vec<&str> = events.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(keys, vec!["4", "5"]);
    }

    #[derive(Debug)]
    struct Noted(Id);

    impl DomainEvent for Noted {
        fn topic(&self) -> Topic {
            topic("test.noted")
        }

        fn key(&self) -> Id {
            self.0.clone()
        }

        fn namespace(&self) -> Namespace {
            Namespace::root()
        }

        fn payload(&self) -> Result<Payload> {
            payload_of(&serde_json::json!({ "id": self.0.to_string() }))
        }
    }

    fn id() -> Id {
        Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    #[test]
    fn drain_empties_the_collector() {
        let mut collector = EventCollector::new();
        collector.collect(Noted(id()));
        collector.collect(Noted(id()));
        assert_eq!(collector.len(), 2);
        assert_eq!(collector.drain().len(), 2);
        assert!(collector.is_empty(), "drain must leave nothing behind");
        assert!(
            collector.drain().is_empty(),
            "a second drain yields nothing"
        );
    }

    #[test]
    fn cloning_an_aggregate_does_not_duplicate_its_pending_events() {
        let mut collector = EventCollector::new();
        collector.collect(Noted(id()));
        assert!(
            collector.clone().is_empty(),
            "a cloned aggregate must not re-emit the original's events"
        );
    }

    #[test]
    fn topic_helper_accepts_dotted_names() {
        assert_eq!(topic("task.completed").as_str(), "task.completed");
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use chrono::{DateTime, Utc};

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn event(topic: &str, key: &str, actor: &str, secs: i64) -> RecordedEvent {
        RecordedEvent {
            topic: topic.to_owned(),
            key: key.to_owned(),
            namespace: "/".to_owned(),
            actor: Some(actor.to_owned()),
            machine: None,
            payload: serde_json::json!({}),
            recorded_at: at(secs),
        }
    }

    const A: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn an_empty_query_matches_everything() {
        assert!(EventQuery::default().matches(&event("task.created", A, "claude", 10)));
    }

    #[test]
    fn topic_prefix_selects_a_whole_family() {
        let query = EventQuery {
            topic_prefix: Some("task.".to_owned()),
            ..Default::default()
        };
        assert!(query.matches(&event("task.created", A, "claude", 10)));
        assert!(query.matches(&event("task.finished", A, "claude", 10)));
        assert!(!query.matches(&event("document.created", A, "claude", 10)));
    }

    #[test]
    fn a_bare_alias_matches_that_alias_on_every_machine() {
        let ev = event("task.claimed", A, "coder-1@01ARZ3NDEKTSV4RRFFQ69G5FAV", 10);
        let by = |actor: &str| EventQuery {
            actor: Some(actor.to_owned()),
            ..Default::default()
        };
        assert!(by("coder-1").matches(&ev));
        assert!(by("coder-1@01ARZ3NDEKTSV4RRFFQ69G5FAV").matches(&ev));
        assert!(!by("coder-1@01BX5ZZKBKACTAV9WEVGEMMVRZ").matches(&ev));
        assert!(!by("coder").matches(&ev));
    }

    #[test]
    fn key_and_actor_and_since_all_narrow() {
        let ev = event("task.created", A, "claude", 100);

        let by_key = EventQuery {
            key: Some(Id::new(A).unwrap()),
            ..Default::default()
        };
        assert!(by_key.matches(&ev));

        let other_key = EventQuery {
            key: Some(Id::new("01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap()),
            ..Default::default()
        };
        assert!(!other_key.matches(&ev));

        let by_actor = EventQuery {
            actor: Some("claude".to_owned()),
            ..Default::default()
        };
        assert!(by_actor.matches(&ev));
        let other_actor = EventQuery {
            actor: Some("codex".to_owned()),
            ..Default::default()
        };
        assert!(!other_actor.matches(&ev));

        let recent = EventQuery {
            since: Some(at(50)),
            ..Default::default()
        };
        assert!(recent.matches(&ev));
        let future = EventQuery {
            since: Some(at(500)),
            ..Default::default()
        };
        assert!(!future.matches(&ev));
    }
}
