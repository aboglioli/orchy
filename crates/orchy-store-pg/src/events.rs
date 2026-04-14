use std::str::FromStr;

use async_trait::async_trait;
use uuid::Uuid;

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
