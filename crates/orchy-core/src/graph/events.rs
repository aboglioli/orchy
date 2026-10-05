use chrono::{DateTime, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

use super::{Edge, Relation};
use crate::entity_ref::EntityRef;
use crate::error::Result;
use crate::event::{DomainEvent, payload_of, topic};
use crate::id::Id;
use crate::namespace::Namespace;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeAdded {
    pub from: EntityRef,
    pub to: EntityRef,
    pub relation: Relation,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeRemoved {
    pub from: EntityRef,
    pub to: EntityRef,
    pub relation: Relation,
    pub at: DateTime<Utc>,
}

impl EdgeAdded {
    pub fn of(edge: &Edge, at: DateTime<Utc>) -> Self {
        Self {
            from: edge.from().clone(),
            to: edge.to().clone(),
            relation: *edge.relation(),
            at,
        }
    }
}

impl EdgeRemoved {
    pub fn of(edge: &Edge, at: DateTime<Utc>) -> Self {
        Self {
            from: edge.from().clone(),
            to: edge.to().clone(),
            relation: *edge.relation(),
            at,
        }
    }
}

impl DomainEvent for EdgeAdded {
    fn topic(&self) -> Topic {
        topic("edge.added")
    }
    fn key(&self) -> Id {
        self.from
            .id()
            .cloned()
            .expect("a link's source is never an actor")
    }
    fn namespace(&self) -> Namespace {
        Namespace::root()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}

impl DomainEvent for EdgeRemoved {
    fn topic(&self) -> Topic {
        topic("edge.removed")
    }
    fn key(&self) -> Id {
        self.from
            .id()
            .cloned()
            .expect("a link's source is never an actor")
    }
    fn namespace(&self) -> Namespace {
        Namespace::root()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}
