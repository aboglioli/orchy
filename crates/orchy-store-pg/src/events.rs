use async_trait::async_trait;
use sqlx::{PgConnection, PgPool, Postgres, Transaction};
use std::collections::HashMap;
use tokio::sync::Mutex;
use uuid::Uuid;

use orchy_events::io::Writer;
use orchy_events::{Error as EventError, Event, Result as EventResult, SerializedEvent};

pub struct PgEventWriter {
    pool: PgPool,
}

pub struct PgTxEventWriter<'tx> {
    tx: Mutex<&'tx mut PgConnection>,
}

impl PgEventWriter {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn new_tx<'a, 'c>(tx: &'a mut Transaction<'c, Postgres>) -> PgTxEventWriter<'a> {
        PgTxEventWriter {
            tx: Mutex::new(&mut **tx),
        }
    }
}

fn serialize_event(event: &Event) -> EventResult<(Uuid, SerializedEvent)> {
    let serialized =
        SerializedEvent::from_event(event).map_err(|e| EventError::Store(e.to_string()))?;
    let id = Uuid::parse_str(&serialized.id).map_err(|e| EventError::Store(e.to_string()))?;
    Ok((id, serialized))
}

fn serialize_metadata(metadata: &HashMap<String, String>) -> EventResult<serde_json::Value> {
    serde_json::to_value(metadata)
        .map_err(|e| EventError::Store(format!("failed to serialize metadata: {e}")))
}

async fn append_to_pool(pool: &PgPool, event: &Event) -> EventResult<()> {
    let (id, serialized) = serialize_event(event)?;

    sqlx::query(
        "INSERT INTO events (id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(&serialized.organization)
    .bind(&serialized.namespace)
    .bind(&serialized.topic)
    .bind(&serialized.key)
    .bind(&serialized.payload)
    .bind(&serialized.content_type)
    .bind(serialize_metadata(&serialized.metadata)?)
    .bind(serialized.timestamp)
    .bind(serialized.version as i64)
    .execute(pool)
    .await
    .map_err(|e| EventError::Store(e.to_string()))?;

    Ok(())
}

async fn append_to_tx(conn: &mut PgConnection, event: &Event) -> EventResult<()> {
    let (id, serialized) = serialize_event(event)?;

    sqlx::query(
        "INSERT INTO events (id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
    )
    .bind(id)
    .bind(&serialized.organization)
    .bind(&serialized.namespace)
    .bind(&serialized.topic)
    .bind(&serialized.key)
    .bind(&serialized.payload)
    .bind(&serialized.content_type)
    .bind(serialize_metadata(&serialized.metadata)?)
    .bind(serialized.timestamp)
    .bind(serialized.version as i64)
    .execute(conn)
    .await
    .map_err(|e| EventError::Store(e.to_string()))?;

    Ok(())
}

#[async_trait]
impl Writer for PgEventWriter {
    async fn write(&self, event: &Event) -> EventResult<()> {
        append_to_pool(&self.pool, event).await
    }

    async fn write_all(&self, events: &[Event]) -> EventResult<()> {
        if events.is_empty() {
            return Ok(());
        }
        let rows: Vec<(Uuid, SerializedEvent, serde_json::Value)> = events
            .iter()
            .map(|e| {
                let (id, ser) = serialize_event(e)?;
                let metadata = serialize_metadata(&ser.metadata)?;
                Ok((id, ser, metadata))
            })
            .collect::<EventResult<_>>()?;

        let mut builder = sqlx::QueryBuilder::new(
            "INSERT INTO events (id, organization, namespace, topic, key, payload, content_type, metadata, timestamp, version) ",
        );
        builder.push_values(rows.iter(), |mut b, (id, ser, metadata)| {
            b.push_bind(id)
                .push_bind(&ser.organization)
                .push_bind(&ser.namespace)
                .push_bind(&ser.topic)
                .push_bind(&ser.key)
                .push_bind(&ser.payload)
                .push_bind(&ser.content_type)
                .push_bind(metadata)
                .push_bind(ser.timestamp)
                .push_bind(ser.version as i64);
        });
        builder
            .build()
            .execute(&self.pool)
            .await
            .map_err(|e| EventError::Store(e.to_string()))?;
        Ok(())
    }
}

#[async_trait]
impl<'tx> Writer for PgTxEventWriter<'tx> {
    async fn write(&self, event: &Event) -> EventResult<()> {
        let mut tx = self.tx.lock().await;
        append_to_tx(*tx, event).await
    }
}
