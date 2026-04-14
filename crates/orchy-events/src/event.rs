use std::fmt;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::Result;
use crate::metadata::Metadata;
use crate::namespace::EventNamespace;
use crate::organization::Organization;
use crate::payload::Payload;
use crate::topic::Topic;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for EventId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone)]
pub struct Event {
    id: EventId,
    organization: Organization,
    namespace: EventNamespace,
    topic: Topic,
    payload: Payload,
    metadata: Metadata,
    timestamp: DateTime<Utc>,
    version: u64,
}

impl Event {
    pub fn create(
        organization: impl Into<String>,
        namespace: impl Into<String>,
        topic: impl Into<String>,
        payload: Payload,
    ) -> Result<Self> {
        Ok(Self {
            id: EventId::new(),
            organization: Organization::new(organization)?,
            namespace: EventNamespace::new(namespace)?,
            topic: Topic::new(topic)?,
            payload,
            metadata: Metadata::new(),
            timestamp: Utc::now(),
            version: 1,
        })
    }

    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    pub fn restore(r: RestoreEvent) -> Self {
        Self {
            id: r.id,
            organization: r.organization,
            namespace: r.namespace,
            topic: r.topic,
            payload: r.payload,
            metadata: r.metadata,
            timestamp: r.timestamp,
            version: r.version,
        }
    }

    pub fn id(&self) -> EventId {
        self.id
    }
    pub fn organization(&self) -> &Organization {
        &self.organization
    }
    pub fn namespace(&self) -> &EventNamespace {
        &self.namespace
    }
    pub fn topic(&self) -> &Topic {
        &self.topic
    }
    pub fn payload(&self) -> &Payload {
        &self.payload
    }
    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }
    pub fn version(&self) -> u64 {
        self.version
    }
}

pub struct RestoreEvent {
    pub id: EventId,
    pub organization: Organization,
    pub namespace: EventNamespace,
    pub topic: Topic,
    pub payload: Payload,
    pub metadata: Metadata,
    pub timestamp: DateTime<Utc>,
    pub version: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_event() {
        let payload = Payload::from_json(&serde_json::json!({"task_id": "123"})).unwrap();
        let event = Event::create("my-project", "task", "task.created", payload).unwrap();
        assert_eq!(event.organization().as_str(), "my-project");
        assert_eq!(event.namespace().as_str(), "task");
        assert_eq!(event.topic().as_str(), "task.created");
        assert_eq!(event.version(), 1);
    }

    #[test]
    fn create_with_metadata() {
        let payload = Payload::from_string("test");
        let metadata = Metadata::new()
            .with("agent_id", "abc-123")
            .with("project", "my-project");
        let event = Event::create("org", "agent", "agent.registered", payload)
            .unwrap()
            .with_metadata(metadata);
        assert_eq!(event.metadata().get("agent_id"), Some("abc-123"));
        assert_eq!(event.metadata().get("project"), Some("my-project"));
    }
}
