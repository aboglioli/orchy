use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};

use crate::error::{Error, Result};
use crate::event::{Event, EventId, RestoreEvent};
use crate::metadata::Metadata;
use crate::namespace::EventNamespace;
use crate::organization::Organization;
use crate::payload::{ContentType, Payload};
use crate::topic::Topic;

#[derive(Debug, Clone)]
pub struct SerializedEvent {
    pub id: String,
    pub organization: String,
    pub namespace: String,
    pub topic: String,
    pub payload: serde_json::Value,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
    pub version: u64,
}

impl SerializedEvent {
    pub fn from_event(event: &Event) -> Result<Self> {
        let payload_value = match event.payload().content_type() {
            ContentType::Json => {
                serde_json::from_slice(event.payload().data())
                    .map_err(|e| Error::Serialization(e.to_string()))?
            }
            _ => serde_json::Value::String(
                String::from_utf8_lossy(event.payload().data()).into_owned(),
            ),
        };

        Ok(Self {
            id: event.id().to_string(),
            organization: event.organization().to_string(),
            namespace: event.namespace().to_string(),
            topic: event.topic().to_string(),
            payload: payload_value,
            content_type: event.payload().content_type().to_string(),
            metadata: event.metadata().as_map().clone(),
            timestamp: event.timestamp(),
            version: event.version(),
        })
    }

    pub fn to_event(&self) -> Result<Event> {
        let content_type: ContentType = self.content_type.parse()
            .map_err(|e: Error| Error::Serialization(e.to_string()))?;

        let data = match content_type {
            ContentType::Json => {
                serde_json::to_vec(&self.payload)
                    .map_err(|e| Error::Serialization(e.to_string()))?
            }
            _ => {
                self.payload.as_str()
                    .unwrap_or("")
                    .as_bytes()
                    .to_vec()
            }
        };

        Ok(Event::restore(RestoreEvent {
            id: EventId::from_str(&self.id)
                .map_err(|e| Error::Serialization(e.to_string()))?,
            organization: Organization::new(&self.organization)?,
            namespace: EventNamespace::new(&self.namespace)?,
            topic: Topic::new(&self.topic)?,
            payload: Payload::from_raw(data, content_type),
            metadata: Metadata::from(self.metadata.clone()),
            timestamp: self.timestamp,
            version: self.version,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let payload = Payload::from_json(&serde_json::json!({"key": "value"})).unwrap();
        let event = Event::create("org", "task", "task.created", payload).unwrap();

        let serialized = SerializedEvent::from_event(&event).unwrap();
        assert_eq!(serialized.topic, "task.created");
        assert_eq!(serialized.namespace, "task");
        assert_eq!(serialized.organization, "org");

        let restored = serialized.to_event().unwrap();
        assert_eq!(restored.topic().as_str(), "task.created");
        assert_eq!(restored.id(), event.id());
    }
}
