use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_core::error::{Error, Result};
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
                        payload: serde_json::from_str(&payload_str).unwrap_or_default(),
                        content_type: row.get(5)?,
                        metadata: serde_json::from_str(&metadata_str).unwrap_or_default(),
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
