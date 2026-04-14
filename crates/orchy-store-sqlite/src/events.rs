use orchy_events::{Error, Result};
use orchy_events::{EventFilter, EventStore, SerializedEvent};

use crate::SqliteBackend;

impl EventStore for SqliteBackend {
    async fn append(&self, events: &[SerializedEvent]) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        for event in events {
            conn.execute(
                "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                rusqlite::params![
                    event.id,
                    event.organization,
                    event.namespace,
                    event.topic,
                    serde_json::to_string(&event.payload).unwrap(),
                    event.content_type,
                    serde_json::to_string(&event.metadata).unwrap(),
                    event.timestamp.to_rfc3339(),
                    event.version,
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }
        Ok(())
    }

    async fn list(&self, filter: EventFilter) -> Result<Vec<SerializedEvent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, organization, namespace, topic, payload, content_type, metadata, timestamp, version
             FROM events WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref org) = filter.organization {
            sql.push_str(&format!(" AND organization = ?{idx}"));
            params.push(Box::new(org.clone()));
            idx += 1;
        }
        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(" AND namespace = ?{idx}"));
            params.push(Box::new(ns.clone()));
            idx += 1;
        }
        if let Some(ref topic) = filter.topic {
            sql.push_str(&format!(" AND topic = ?{idx}"));
            params.push(Box::new(topic.clone()));
            idx += 1;
        }
        if let Some(ref since) = filter.since {
            sql.push_str(&format!(" AND timestamp >= ?{idx}"));
            params.push(Box::new(since.to_rfc3339()));
            idx += 1;
        }
        let _ = idx;

        sql.push_str(" ORDER BY timestamp ASC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();

        let rows = stmt
            .query_map(param_refs.as_slice(), |row| {
                let id: String = row.get(0)?;
                let organization: String = row.get(1)?;
                let namespace: String = row.get(2)?;
                let topic: String = row.get(3)?;
                let payload_str: String = row.get(4)?;
                let content_type: String = row.get(5)?;
                let metadata_str: String = row.get(6)?;
                let timestamp_str: String = row.get(7)?;
                let version: u64 = row.get(8)?;

                let payload: serde_json::Value =
                    serde_json::from_str(&payload_str).unwrap_or_default();
                let metadata: std::collections::HashMap<String, String> =
                    serde_json::from_str(&metadata_str).unwrap_or_default();
                let timestamp = chrono::DateTime::parse_from_rfc3339(&timestamp_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_default();

                Ok(SerializedEvent {
                    id,
                    organization,
                    namespace,
                    topic,
                    payload,
                    content_type,
                    metadata,
                    timestamp,
                    version,
                })
            })
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows)
    }
}
