use orchy_core::error::Result;
use orchy_events::io::Writer;
use orchy_events::{Error as EventsError, Event, Result as EventsResult, SerializedEvent};

use std::sync::Arc;

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
    // seq is auto-assigned via MAX(seq)+1; safe because Arc<Mutex<Connection>>
    // serializes all writes within the process. SQLite is single-writer per file.
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

impl Writer for SqliteEventWriter {
    async fn write(&self, event: &Event) -> EventsResult<()> {
        let conn = Arc::clone(&self.conn);
        let event = event.clone();
        tokio::task::spawn_blocking(move || {
            let guard = conn.lock().map_err(|e| EventsError::Store(e.to_string()))?;
            append_event(&guard, &event)
        })
        .await
        .map_err(|e| EventsError::Store(format!("blocking task panicked: {e}")))?
    }

    async fn write_all(&self, events: &[Event]) -> EventsResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let conn = Arc::clone(&self.conn);
        let events = events.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut guard = conn.lock().map_err(|e| EventsError::Store(e.to_string()))?;
            let tx = guard
                .transaction()
                .map_err(|e| EventsError::Store(e.to_string()))?;
            for event in &events {
                append_event(&tx, event)?;
            }
            tx.commit().map_err(|e| EventsError::Store(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| EventsError::Store(format!("blocking task panicked: {e}")))?
    }
}
