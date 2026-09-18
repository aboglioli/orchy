use chrono::{DateTime, Utc};
use eventuary_core::{Payload, Topic};
use serde::{Deserialize, Serialize};

use super::status::TaskStatus;
use crate::actor::ActorId;
use crate::error::Result;
use crate::event::{DomainEvent, payload_of, topic};
use crate::id::Id;
use crate::namespace::Namespace;

macro_rules! task_event {
    ($name:ident, $topic:literal) => {
        impl DomainEvent for $name {
            fn topic(&self) -> Topic {
                topic($topic)
            }

            fn key(&self) -> Id {
                self.id.clone()
            }

            fn namespace(&self) -> Namespace {
                self.namespace.clone()
            }

            fn payload(&self) -> Result<Payload> {
                payload_of(self)
            }
        }
    };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskCreated {
    pub id: Id,
    pub namespace: Namespace,
    pub title: String,
    pub parent: Option<Id>,
    pub at: DateTime<Utc>,
}
task_event!(TaskCreated, "task.created");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskClaimed {
    pub id: Id,
    pub namespace: Namespace,
    pub by: ActorId,
    pub at: DateTime<Utc>,
}
task_event!(TaskClaimed, "task.claimed");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReleased {
    pub id: Id,
    pub namespace: Namespace,
    pub by: ActorId,
    pub at: DateTime<Utc>,
}
task_event!(TaskReleased, "task.released");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStarted {
    pub id: Id,
    pub namespace: Namespace,
    pub at: DateTime<Utc>,
}
task_event!(TaskStarted, "task.started");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskFinished {
    pub id: Id,
    pub namespace: Namespace,
    pub status: TaskStatus,
    pub note: Option<String>,
    pub at: DateTime<Utc>,
}
task_event!(TaskFinished, "task.finished");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRolledUp {
    pub id: Id,
    pub namespace: Namespace,
    pub status: TaskStatus,
    pub because: String,
    pub at: DateTime<Utc>,
}
task_event!(TaskRolledUp, "task.rolled_up");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskBlocked {
    pub id: Id,
    pub namespace: Namespace,
    pub reason: String,
    pub at: DateTime<Utc>,
}
task_event!(TaskBlocked, "task.blocked");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUnblocked {
    pub id: Id,
    pub namespace: Namespace,
    pub at: DateTime<Utc>,
}
task_event!(TaskUnblocked, "task.unblocked");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskReparented {
    pub id: Id,
    pub namespace: Namespace,
    pub parent: Option<Id>,
    pub at: DateTime<Utc>,
}
task_event!(TaskReparented, "task.reparented");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskUpdated {
    pub id: Id,
    pub namespace: Namespace,
    pub field: String,
    pub at: DateTime<Utc>,
}
task_event!(TaskUpdated, "task.updated");

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Id {
        Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    #[test]
    fn every_task_topic_is_valid_and_namespaced_under_task() {
        let at = Utc::now();
        let events: Vec<Box<dyn DomainEvent>> = vec![
            Box::new(TaskCreated {
                id: id(),
                namespace: Namespace::root(),
                title: "t".to_owned(),
                parent: None,
                at,
            }),
            Box::new(TaskClaimed {
                id: id(),
                namespace: Namespace::root(),
                by: ActorId::new("claude", "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
                at,
            }),
            Box::new(TaskStarted {
                id: id(),
                namespace: Namespace::root(),
                at,
            }),
            Box::new(TaskFinished {
                id: id(),
                namespace: Namespace::root(),
                status: TaskStatus::Completed,
                note: None,
                at,
            }),
            Box::new(TaskRolledUp {
                id: id(),
                namespace: Namespace::root(),
                status: TaskStatus::Completed,
                because: "x".to_owned(),
                at,
            }),
            Box::new(TaskBlocked {
                id: id(),
                namespace: Namespace::root(),
                reason: "x".to_owned(),
                at,
            }),
            Box::new(TaskUnblocked {
                id: id(),
                namespace: Namespace::root(),
                at,
            }),
            Box::new(TaskReparented {
                id: id(),
                namespace: Namespace::root(),
                parent: None,
                at,
            }),
            Box::new(TaskUpdated {
                id: id(),
                namespace: Namespace::root(),
                field: "title".to_owned(),
                at,
            }),
        ];
        for event in events {
            assert!(
                event.topic().as_str().starts_with("task."),
                "{:?}",
                event.topic()
            );
            assert!(event.payload().is_ok(), "payload must serialize: {event:?}");
            assert_eq!(event.key(), id());
        }
    }

    #[test]
    fn rollup_is_a_distinct_topic_from_an_agents_own_completion() {
        let at = Utc::now();
        let finished = TaskFinished {
            id: id(),
            namespace: Namespace::root(),
            status: TaskStatus::Completed,
            note: None,
            at,
        };
        let rolled = TaskRolledUp {
            id: id(),
            namespace: Namespace::root(),
            status: TaskStatus::Completed,
            because: "x".to_owned(),
            at,
        };
        assert_ne!(
            finished.topic().as_str(),
            rolled.topic().as_str(),
            "replay must be able to tell a human decision from a derivation"
        );
    }
}
