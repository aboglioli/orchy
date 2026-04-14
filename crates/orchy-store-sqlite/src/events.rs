use async_trait::async_trait;

use orchy_events::io::Writer;
use orchy_events::{Event, SerializedEvent};

use crate::SqliteBackend;

#[async_trait]
impl Writer for SqliteBackend {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let serialized = SerializedEvent::from_event(event)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                serialized.id,
                serialized.organization,
                serialized.namespace,
                serialized.topic,
                serde_json::to_string(&serialized.payload).unwrap(),
                serialized.content_type,
                serde_json::to_string(&serialized.metadata).unwrap(),
                serialized.timestamp.to_rfc3339(),
                serialized.version,
            ],
        )
        .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        Ok(())
    }
}
