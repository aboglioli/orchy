use chrono::{DateTime, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

use super::Role;
use super::identity::ActorId;
use super::lease::ResourceKey;
use crate::error::Result;
use crate::event::{DomainEvent, payload_of, topic};
use crate::id::Id;
use crate::namespace::Namespace;

/// Keyed by the machine: an actor id (`alias@machine`) is not a ULID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorAnnounced {
    pub actor: ActorId,
    pub namespace: Namespace,
    pub roles: Vec<Role>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorUpdated {
    pub actor: ActorId,
    pub namespace: Namespace,
    pub field: String,
    pub at: DateTime<Utc>,
}

impl DomainEvent for ActorAnnounced {
    fn topic(&self) -> Topic {
        topic("actor.announced")
    }
    fn key(&self) -> Id {
        self.actor.machine().id().clone()
    }
    fn namespace(&self) -> Namespace {
        self.namespace.clone()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}

impl DomainEvent for ActorUpdated {
    fn topic(&self) -> Topic {
        topic("actor.updated")
    }
    fn key(&self) -> Id {
        self.actor.machine().id().clone()
    }
    fn namespace(&self) -> Namespace {
        self.namespace.clone()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseChange {
    Acquired,
    Renewed,
    Released,
}

/// Keyed by the holder's machine: a resource name is free text, not a ULID.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaseChanged {
    pub change: LeaseChange,
    pub resource: ResourceKey,
    pub holder: ActorId,
    pub expires_at: Option<DateTime<Utc>>,
    pub at: DateTime<Utc>,
}

impl DomainEvent for LeaseChanged {
    fn topic(&self) -> Topic {
        topic(match self.change {
            LeaseChange::Acquired => "lock.acquired",
            LeaseChange::Renewed => "lock.renewed",
            LeaseChange::Released => "lock.released",
        })
    }
    fn key(&self) -> Id {
        self.holder.machine().id().clone()
    }
    fn namespace(&self) -> Namespace {
        Namespace::root()
    }
    fn payload(&self) -> Result<Payload> {
        payload_of(self)
    }
}
