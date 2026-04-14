use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::error::{Error, Result};
use orchy_events::io::Writer;
use orchy_events::{Event, SerializedEvent};

use crate::PgBackend;

#[async_trait]
impl Writer for PgBackend {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let serialized = SerializedEvent::from_event(event)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        let id = Uuid::from_str(&serialized.id)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(id)
        .bind(&serialized.organization)
        .bind(&serialized.namespace)
        .bind(&serialized.topic)
        .bind(&serialized.payload)
        .bind(&serialized.content_type)
        .bind(serde_json::to_value(&serialized.metadata).unwrap())
        .bind(serialized.timestamp)
        .bind(serialized.version as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| orchy_events::Error::Store(e.to_string()))?;

        Ok(())
    }
}

impl PgBackend {
    pub async fn query_events(
        &self,
        organization: &str,
        since: DateTime<Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        let rows = sqlx::query(
            "SELECT id, organization, namespace, topic, payload, content_type, metadata, timestamp, version
             FROM events
             WHERE organization = $1 AND timestamp >= $2
             ORDER BY timestamp DESC
             LIMIT $3",
        )
        .bind(organization)
        .bind(since)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut events: Vec<SerializedEvent> = rows
            .iter()
            .map(|row| {
                let id: Uuid = row.get("id");
                let metadata_json: serde_json::Value = row.get("metadata");
                SerializedEvent {
                    id: id.to_string(),
                    organization: row.get("organization"),
                    namespace: row.get("namespace"),
                    topic: row.get("topic"),
                    payload: row.get("payload"),
                    content_type: row.get("content_type"),
                    metadata: serde_json::from_value(metadata_json).unwrap_or_default(),
                    timestamp: row.get("timestamp"),
                    version: row.get::<i64, _>("version") as u64,
                }
            })
            .collect();
        events.reverse();
        Ok(events)
    }
}
