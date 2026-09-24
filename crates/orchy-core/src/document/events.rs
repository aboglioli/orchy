use chrono::{DateTime, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

use super::kind::{DocumentStatus, Kind};
use crate::error::Result;
use crate::event::{DomainEvent, payload_of, topic};
use crate::id::Id;
use crate::namespace::Namespace;

macro_rules! document_event {
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
pub struct DocumentCreated {
    pub id: Id,
    pub namespace: Namespace,
    pub kind: Kind,
    pub title: String,
    pub content_hash: String,
    pub at: DateTime<Utc>,
}
document_event!(DocumentCreated, "document.created");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentWritten {
    pub id: Id,
    pub namespace: Namespace,
    pub content_hash: String,
    pub prev_hash: String,
    pub at: DateTime<Utc>,
}
document_event!(DocumentWritten, "document.written");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSectionReplaced {
    pub id: Id,
    pub namespace: Namespace,
    pub heading: String,
    pub content_hash: String,
    pub prev_hash: String,
    pub at: DateTime<Utc>,
}
document_event!(DocumentSectionReplaced, "document.section_replaced");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentFieldSet {
    pub id: Id,
    pub namespace: Namespace,
    pub field: String,
    pub value: serde_json::Value,
    pub at: DateTime<Utc>,
}
document_event!(DocumentFieldSet, "document.field_set");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMoved {
    pub id: Id,
    pub namespace: Namespace,
    pub from: Namespace,
    pub at: DateTime<Utc>,
}
document_event!(DocumentMoved, "document.moved");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRetyped {
    pub id: Id,
    pub namespace: Namespace,
    pub from: Kind,
    pub to: Kind,
    pub at: DateTime<Utc>,
}
document_event!(DocumentRetyped, "document.retyped");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSuperseded {
    pub id: Id,
    pub namespace: Namespace,
    pub by: Id,
    pub at: DateTime<Utc>,
}
document_event!(DocumentSuperseded, "document.superseded");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStatusChanged {
    pub id: Id,
    pub namespace: Namespace,
    pub status: DocumentStatus,
    pub at: DateTime<Utc>,
}
document_event!(DocumentStatusChanged, "document.status_changed");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentPromoted {
    pub id: Id,
    pub namespace: Namespace,
    pub from: Namespace,
    pub at: DateTime<Utc>,
}
document_event!(DocumentPromoted, "document.promoted");

#[cfg(test)]
mod tests {
    use super::*;

    fn id() -> Id {
        Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    #[test]
    fn every_document_topic_is_prefixed_and_serializes() {
        let at = Utc::now();
        let ns = Namespace::root();
        let kind = Kind::Decision;
        let events: Vec<Box<dyn DomainEvent>> = vec![
            Box::new(DocumentCreated {
                id: id(),
                namespace: ns.clone(),
                kind,
                title: "t".to_owned(),
                content_hash: "h".to_owned(),
                at,
            }),
            Box::new(DocumentWritten {
                id: id(),
                namespace: ns.clone(),
                content_hash: "h".to_owned(),
                prev_hash: "g".to_owned(),
                at,
            }),
            Box::new(DocumentSectionReplaced {
                id: id(),
                namespace: ns.clone(),
                heading: "H".to_owned(),
                content_hash: "h".to_owned(),
                prev_hash: "g".to_owned(),
                at,
            }),
            Box::new(DocumentFieldSet {
                id: id(),
                namespace: ns.clone(),
                field: "f".to_owned(),
                value: serde_json::json!(1),
                at,
            }),
            Box::new(DocumentMoved {
                id: id(),
                namespace: ns.clone(),
                from: ns.clone(),
                at,
            }),
            Box::new(DocumentRetyped {
                id: id(),
                namespace: ns.clone(),
                from: kind,
                to: kind,
                at,
            }),
            Box::new(DocumentSuperseded {
                id: id(),
                namespace: ns.clone(),
                by: id(),
                at,
            }),
            Box::new(DocumentStatusChanged {
                id: id(),
                namespace: ns.clone(),
                status: DocumentStatus::Archived,
                at,
            }),
            Box::new(DocumentPromoted {
                id: id(),
                namespace: ns.clone(),
                from: ns.clone(),
                at,
            }),
        ];
        for event in events {
            assert!(
                event.topic().as_str().starts_with("document."),
                "{:?}",
                event.topic()
            );
            assert!(event.payload().is_ok(), "{event:?}");
        }
    }

    #[test]
    fn content_changing_events_carry_a_hash_chain() {
        let at = Utc::now();
        let written = DocumentWritten {
            id: id(),
            namespace: Namespace::root(),
            content_hash: "new".to_owned(),
            prev_hash: "old".to_owned(),
            at,
        };
        let json = written.payload().unwrap();
        let text = String::from_utf8(json.data().to_vec()).unwrap();
        assert!(text.contains("prev_hash"), "fork detection needs the chain");
        assert!(text.contains("content_hash"));
    }
}
