use std::collections::HashMap;
use std::str::FromStr;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::event::{Event, EventId, RestoreEvent};
use crate::event_key::EventKey;
use crate::metadata::Metadata;
use crate::namespace::Namespace;
use crate::organization::OrganizationId;
use crate::payload::{ContentType, Payload};
use crate::topic::Topic;

const BINARY_BASE64_KEY: &str = "__binary_base64";

#[derive(Debug, Clone)]
pub struct SerializedEvent {
    pub id: String,
    pub organization: String,
    pub namespace: String,
    pub topic: String,
    pub key: String,
    pub payload: Value,
    pub content_type: String,
    pub metadata: HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
    pub version: u64,
}

impl SerializedEvent {
    pub fn from_event(event: &Event) -> Result<Self> {
        let payload_value = match event.payload().content_type() {
            ContentType::Json => serde_json::from_slice(event.payload().data())
                .map_err(|e| Error::Serialization(e.to_string()))?,
            ContentType::PlainText => {
                let s = std::str::from_utf8(event.payload().data()).map_err(|_| {
                    Error::Serialization("invalid UTF-8 in plain text payload".into())
                })?;
                Value::String(s.to_owned())
            }
            ContentType::Binary => {
                let encoded = BASE64.encode(event.payload().data());
                json!({ BINARY_BASE64_KEY: encoded })
            }
        };

        Ok(Self {
            id: event.id().to_string(),
            organization: event.organization().to_string(),
            namespace: event.namespace().to_string(),
            topic: event.topic().to_string(),
            key: event.key().to_string(),
            payload: payload_value,
            content_type: event.payload().content_type().to_string(),
            metadata: event.metadata().as_map().clone(),
            timestamp: event.timestamp(),
            version: event.version(),
        })
    }

    pub fn to_event(&self) -> Result<Event> {
        let content_type: ContentType = self
            .content_type
            .parse()
            .map_err(|e: Error| Error::Serialization(e.to_string()))?;

        let data = match content_type {
            ContentType::Json => serde_json::to_vec(&self.payload)
                .map_err(|e| Error::Serialization(e.to_string()))?,
            ContentType::PlainText => self
                .payload
                .as_str()
                .ok_or_else(|| Error::Serialization("expected string for plain text".into()))?
                .as_bytes()
                .to_vec(),
            ContentType::Binary => {
                let encoded = self
                    .payload
                    .get(BINARY_BASE64_KEY)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        Error::Serialization(format!(
                            "missing {BINARY_BASE64_KEY} for binary payload"
                        ))
                    })?;
                BASE64
                    .decode(encoded)
                    .map_err(|e| Error::Serialization(format!("invalid base64: {e}")))?
            }
        };

        Ok(Event::restore(RestoreEvent {
            id: EventId::from_str(&self.id).map_err(|e| Error::Serialization(e.to_string()))?,
            organization: OrganizationId::new(&self.organization)?,
            namespace: Namespace::new(&self.namespace)?,
            topic: Topic::new(&self.topic)?,
            key: EventKey::new(&self.key)?,
            payload: Payload::from_raw(data, content_type),
            metadata: Metadata::from(self.metadata.clone()),
            timestamp: self.timestamp,
            version: self.version,
        }))
    }

    pub fn to_json_value(&self) -> Value {
        json!({
            "id": self.id,
            "organization": self.organization,
            "namespace": self.namespace,
            "topic": self.topic,
            "key": self.key,
            "payload": self.payload,
            "content_type": self.content_type,
            "metadata": self.metadata,
            "timestamp": self.timestamp.to_rfc3339(),
            "version": self.version,
        })
    }

    pub fn to_json_string(&self) -> Result<String> {
        serde_json::to_string(&self.to_json_value())
            .map_err(|e| Error::Serialization(e.to_string()))
    }

    pub fn from_json_str(s: &str) -> Result<Self> {
        let value = serde_json::from_str(s).map_err(|e| Error::Serialization(e.to_string()))?;
        Self::from_json_value(&value)
    }

    pub fn from_json_slice(bytes: &[u8]) -> Result<Self> {
        let value =
            serde_json::from_slice(bytes).map_err(|e| Error::Serialization(e.to_string()))?;
        Self::from_json_value(&value)
    }

    pub fn from_json_value(value: &Value) -> Result<Self> {
        fn required_string(value: &Value, field: &str) -> Result<String> {
            value
                .get(field)
                .and_then(|v| v.as_str())
                .map(str::to_owned)
                .ok_or_else(|| Error::Serialization(format!("missing or invalid {field}")))
        }

        let timestamp_str = required_string(value, "timestamp")?;
        let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
            .map_err(|e| Error::Serialization(format!("invalid timestamp: {e}")))?
            .with_timezone(&Utc);

        let metadata = value
            .get("metadata")
            .cloned()
            .ok_or_else(|| Error::Serialization("missing metadata".into()))?;

        Ok(Self {
            id: required_string(value, "id")?,
            organization: required_string(value, "organization")?,
            namespace: required_string(value, "namespace")?,
            topic: required_string(value, "topic")?,
            key: required_string(value, "key")?,
            payload: value
                .get("payload")
                .cloned()
                .ok_or_else(|| Error::Serialization("missing payload".into()))?,
            content_type: required_string(value, "content_type")?,
            metadata: serde_json::from_value(metadata)
                .map_err(|e| Error::Serialization(format!("invalid metadata: {e}")))?,
            timestamp,
            version: value
                .get("version")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| Error::Serialization("missing or invalid version".into()))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let payload = Payload::from_json(&serde_json::json!({"key": "value"})).unwrap();
        let event = Event::create("org", "/task", "task.created", "task-123", payload).unwrap();

        let serialized = SerializedEvent::from_event(&event).unwrap();
        assert_eq!(serialized.topic, "task.created");
        assert_eq!(serialized.namespace, "/task");
        assert_eq!(serialized.organization, "org");
        assert_eq!(serialized.key, "task-123");

        let restored = serialized.to_event().unwrap();
        assert_eq!(restored.topic().as_str(), "task.created");
        assert_eq!(restored.id(), event.id());
    }

    #[test]
    fn binary_payload_survives_roundtrip() {
        let data = vec![0x00, 0xFF, 0x80, 0x7F, 0x42];
        let payload = Payload::from_bytes(data.clone());
        let event = Event::create("org", "/x", "test.binary", "k", payload).unwrap();

        let serialized = SerializedEvent::from_event(&event).unwrap();
        let restored_event = serialized.to_event().unwrap();

        assert_eq!(restored_event.payload().content_type(), ContentType::Binary);
        assert_eq!(restored_event.payload().data(), &data);
    }

    fn serialized_event_with(content_type: &str, payload: serde_json::Value) -> SerializedEvent {
        SerializedEvent {
            id: "01900000-0000-7000-8000-000000000000".to_owned(),
            organization: "org".to_owned(),
            namespace: "/x".to_owned(),
            topic: "test.topic".to_owned(),
            key: "k".to_owned(),
            payload,
            content_type: content_type.to_owned(),
            metadata: HashMap::new(),
            timestamp: Utc::now(),
            version: 1,
        }
    }

    #[test]
    fn plain_text_payload_with_invalid_utf8_errors_on_serialize() {
        let serialized = serialized_event_with("text/plain", serde_json::json!(42));

        let err = serialized.to_event().unwrap_err();
        match err {
            Error::Serialization(msg) => {
                assert!(
                    msg.contains("expected string for plain text"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn binary_payload_missing_base64_key_errors_on_deserialize() {
        let serialized = serialized_event_with("application/octet-stream", serde_json::json!({}));

        let err = serialized.to_event().unwrap_err();
        match err {
            Error::Serialization(msg) => {
                assert!(
                    msg.contains("missing __binary_base64"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn binary_payload_invalid_base64_errors_on_deserialize() {
        let serialized = serialized_event_with(
            "application/octet-stream",
            serde_json::json!({ "__binary_base64": "!!!not-base64!!!" }),
        );

        let err = serialized.to_event().unwrap_err();
        match err {
            Error::Serialization(msg) => {
                assert!(
                    msg.contains("invalid base64"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn json_value_roundtrip() {
        let payload = Payload::from_json(&serde_json::json!({"x": true})).unwrap();
        let metadata = Metadata::new().with("trace-id", "abc-123");
        let event = Event::create("org", "/ns", "topic.test", "k1", payload)
            .unwrap()
            .with_metadata(metadata);
        let s = SerializedEvent::from_event(&event).unwrap();

        let json_val = s.to_json_value();
        let restored = SerializedEvent::from_json_value(&json_val).unwrap();

        assert_eq!(restored.id, s.id);
        assert_eq!(restored.organization, s.organization);
        assert_eq!(restored.namespace, s.namespace);
        assert_eq!(restored.topic, s.topic);
        assert_eq!(restored.key, s.key);
        assert_eq!(restored.payload, s.payload);
        assert_eq!(restored.content_type, s.content_type);
        assert_eq!(restored.metadata, s.metadata);
        assert_eq!(restored.timestamp, s.timestamp);
        assert_eq!(restored.version, s.version);
    }

    fn valid_event_json() -> Value {
        json!({
            "id": "01900000-0000-7000-8000-000000000000",
            "organization": "org",
            "namespace": "/ns",
            "topic": "topic.test",
            "key": "k1",
            "payload": { "x": true },
            "content_type": "application/json",
            "metadata": { "trace-id": "abc-123" },
            "timestamp": "2026-01-01T00:00:00Z",
            "version": 1,
        })
    }

    #[test]
    fn json_string_roundtrip() {
        let payload = Payload::from_json(&serde_json::json!({"x": true})).unwrap();
        let metadata = Metadata::new().with("trace-id", "abc-123");
        let event = Event::create("org", "/ns", "topic.test", "k1", payload)
            .unwrap()
            .with_metadata(metadata);
        let s = SerializedEvent::from_event(&event).unwrap();

        let json_str = s.to_json_string().unwrap();
        let restored = SerializedEvent::from_json_str(&json_str).unwrap();

        assert_eq!(restored.id, s.id);
        assert_eq!(restored.organization, s.organization);
        assert_eq!(restored.namespace, s.namespace);
        assert_eq!(restored.topic, s.topic);
        assert_eq!(restored.key, s.key);
        assert_eq!(restored.payload, s.payload);
        assert_eq!(restored.content_type, s.content_type);
        assert_eq!(restored.metadata, s.metadata);
        assert_eq!(restored.timestamp, s.timestamp);
        assert_eq!(restored.version, s.version);
    }

    #[test]
    fn json_slice_roundtrip() {
        let payload = Payload::from_json(&serde_json::json!({"x": true})).unwrap();
        let event = Event::create("org", "/ns", "topic.test", "k1", payload).unwrap();
        let s = SerializedEvent::from_event(&event).unwrap();

        let json_str = s.to_json_string().unwrap();
        let restored = SerializedEvent::from_json_slice(json_str.as_bytes()).unwrap();

        assert_eq!(restored.id, s.id);
        assert_eq!(restored.timestamp, s.timestamp);
        assert_eq!(restored.version, s.version);
    }

    #[test]
    fn from_json_str_missing_field_errors() {
        let mut value = valid_event_json();
        value.as_object_mut().unwrap().remove("id");
        let raw = serde_json::to_string(&value).unwrap();

        let err = SerializedEvent::from_json_str(&raw).unwrap_err();
        match err {
            Error::Serialization(msg) => {
                assert!(
                    msg.contains("missing or invalid id"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn from_json_slice_invalid_timestamp_errors() {
        let mut value = valid_event_json();
        value["timestamp"] = json!("not-a-date");
        let raw = serde_json::to_vec(&value).unwrap();

        let err = SerializedEvent::from_json_slice(&raw).unwrap_err();
        match err {
            Error::Serialization(msg) => {
                assert!(
                    msg.contains("invalid timestamp"),
                    "unexpected error message: {msg}"
                );
            }
            other => panic!("expected Serialization error, got {other:?}"),
        }
    }

    #[test]
    fn from_json_str_invalid_json_errors() {
        let err = SerializedEvent::from_json_str("not-json{").unwrap_err();
        assert!(matches!(err, Error::Serialization(_)));
    }

    #[test]
    fn from_json_value_rejects_invalid_inputs() {
        type Mutate = Box<dyn Fn(&mut Value)>;
        let cases: Vec<(Mutate, &str)> = vec![
            (
                Box::new(|v| {
                    v.as_object_mut().unwrap().remove("id");
                }),
                "missing or invalid id",
            ),
            (
                Box::new(|v| {
                    v.as_object_mut().unwrap().remove("topic");
                }),
                "missing or invalid topic",
            ),
            (
                Box::new(|v| {
                    v.as_object_mut().unwrap().remove("key");
                }),
                "missing or invalid key",
            ),
            (
                Box::new(|v| {
                    v.as_object_mut().unwrap().remove("payload");
                }),
                "missing payload",
            ),
            (
                Box::new(|v| {
                    v.as_object_mut().unwrap().remove("metadata");
                }),
                "missing metadata",
            ),
            (
                Box::new(|v| {
                    v["metadata"] = json!([]);
                }),
                "invalid metadata",
            ),
            (
                Box::new(|v| {
                    v["timestamp"] = json!("not-a-date");
                }),
                "invalid timestamp",
            ),
            (
                Box::new(|v| {
                    v["version"] = json!("one");
                }),
                "missing or invalid version",
            ),
        ];

        for (mutate, expected) in cases {
            let mut value = valid_event_json();
            mutate(&mut value);
            let err = SerializedEvent::from_json_value(&value).unwrap_err();
            match err {
                Error::Serialization(msg) => {
                    assert!(
                        msg.contains(expected),
                        "expected error to contain {expected:?}, got {msg:?}"
                    );
                }
                other => panic!("expected Serialization error, got {other:?}"),
            }
        }
    }
}
