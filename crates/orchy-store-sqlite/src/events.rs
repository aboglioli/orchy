use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::error::{Error, Result};
use orchy_events::io::Writer;
use orchy_events::{Event, SerializedEvent};

use crate::SqliteBackend;

pub struct SqliteEventWriter<'a> {
    backend: &'a SqliteBackend,
}

impl<'a> SqliteEventWriter<'a> {
    pub fn new(backend: &'a SqliteBackend) -> Self {
        Self { backend }
    }
}

fn serialize_event(event: &Event) -> orchy_events::Result<SerializedEvent> {
    SerializedEvent::from_event(event).map_err(|e| orchy_events::Error::Store(e.to_string()))
}

fn append_event(conn: &rusqlite::Connection, event: &Event) -> orchy_events::Result<()> {
    let serialized = serialize_event(event)?;
    conn.execute(
        "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        rusqlite::params![
            serialized.id,
            serialized.organization,
            serialized.namespace,
            serialized.topic,
            serde_json::to_string(&serialized.payload)
                .map_err(|e| orchy_events::Error::Store(format!("failed to serialize payload: {e}")))?,
            serialized.content_type,
            serde_json::to_string(&serialized.metadata)
                .map_err(|e| orchy_events::Error::Store(format!("failed to serialize metadata: {e}")))?,
            serialized.timestamp.to_rfc3339(),
            serialized.version,
        ],
    )
    .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
    Ok(())
}

pub(crate) fn write_events_in_tx(tx: &rusqlite::Transaction<'_>, events: &[Event]) -> Result<()> {
    for event in events {
        append_event(tx, event).map_err(|e| Error::Store(e.to_string()))?;
    }

    Ok(())
}

#[async_trait]
impl Writer for SqliteEventWriter<'_> {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let conn = self
            .backend
            .conn
            .lock()
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        append_event(&conn, event)
    }
}

#[async_trait]
impl Writer for SqliteBackend {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        SqliteEventWriter::new(self).write(event).await
    }
}

impl SqliteBackend {
    pub fn query_events(
        &self,
        organization: &str,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let sql = "SELECT id, organization, namespace, topic, payload, content_type, metadata, timestamp, version FROM events WHERE organization = ?1 AND timestamp >= ?2 ORDER BY timestamp DESC LIMIT ?3";

        let mut stmt = conn.prepare(sql).map_err(|e| Error::Store(e.to_string()))?;

        let rows = stmt
            .query_map(
                rusqlite::params![organization, since.to_rfc3339(), limit as i64],
                |row| {
                    let payload_str: String = row.get(4)?;
                    let metadata_str: String = row.get(6)?;
                    let timestamp_str: String = row.get(7)?;
                    Ok(SerializedEvent {
                        id: row.get(0)?,
                        organization: row.get(1)?,
                        namespace: row.get(2)?,
                        topic: row.get(3)?,
                        payload: crate::decode_json(&payload_str, "payload")?,
                        content_type: row.get(5)?,
                        metadata: crate::decode_json(&metadata_str, "metadata")?,
                        timestamp: DateTime::parse_from_rfc3339(&timestamp_str)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now()),
                        version: row.get::<_, i64>(8)? as u64,
                    })
                },
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row.map_err(|e| Error::Store(e.to_string()))?);
        }
        events.reverse();
        Ok(events)
    }
}
