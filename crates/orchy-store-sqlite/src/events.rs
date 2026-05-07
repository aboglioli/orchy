use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_events::io::Writer;
use orchy_events::{Error as EventsError, Event, Result as EventsResult, SerializedEvent};

use crate::SqliteConn;

pub struct SqliteEventWriter {
    conn: SqliteConn,
}

impl SqliteEventWriter {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

fn serialize_event(event: &Event) -> EventsResult<SerializedEvent> {
    SerializedEvent::from_event(event).map_err(|e| EventsError::Store(e.to_string()))
}

fn append_event(conn: &rusqlite::Connection, event: &Event) -> EventsResult<()> {
    let serialized = serialize_event(event)?;
    conn.execute(
        "INSERT INTO events (id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version, seq)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, (SELECT COALESCE(MAX(seq), 0) + 1 FROM events))",
        rusqlite::params![
            serialized.id,
            serialized.organization,
            serialized.namespace,
            serialized.topic,
            serialized.key,
            serde_json::to_string(&serialized.payload)
                .map_err(|e| EventsError::Store(format!("failed to serialize payload: {e}")))?,
            serialized.content_type,
            serde_json::to_string(&serialized.metadata)
                .map_err(|e| EventsError::Store(format!("failed to serialize metadata: {e}")))?,
            serialized.timestamp.to_rfc3339(),
            serialized.version,
        ],
    )
    .map_err(|e| EventsError::Store(e.to_string()))?;
    Ok(())
}

pub(crate) fn write_events_in_tx(tx: &rusqlite::Transaction<'_>, events: &[Event]) -> Result<()> {
    for event in events {
        append_event(tx, event)?;
    }

    Ok(())
}

#[async_trait]
impl Writer for SqliteEventWriter {
    async fn write(&self, event: &Event) -> EventsResult<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| EventsError::Store(e.to_string()))?;
        append_event(&conn, event)
    }
}
