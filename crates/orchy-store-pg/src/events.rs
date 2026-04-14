use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_events::{EventFilter, EventStore, SerializedEvent};
use orchy_events::{Error, Result};

use crate::PgBackend;

impl EventStore for PgBackend {
    async fn append(&self, events: &[SerializedEvent]) -> Result<()> {
        for event in events {
            let id = Uuid::from_str(&event.id).map_err(|e| Error::Store(e.to_string()))?;

            sqlx::query(
                "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            )
            .bind(id)
            .bind(&event.organization)
            .bind(&event.namespace)
            .bind(&event.topic)
            .bind(&event.payload)
            .bind(&event.content_type)
            .bind(serde_json::to_value(&event.metadata).unwrap())
            .bind(event.timestamp)
            .bind(event.version as i64)
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }
        Ok(())
    }

    async fn list(&self, filter: EventFilter) -> Result<Vec<SerializedEvent>> {
        let mut sql = String::from(
            "SELECT id, organization, namespace, topic, payload, content_type, metadata, timestamp, version
             FROM events WHERE 1=1",
        );
        let mut param_idx = 1u32;
        let mut org_val: Option<String> = None;
        let mut ns_val: Option<String> = None;
        let mut topic_val: Option<String> = None;
        let mut since_val: Option<DateTime<Utc>> = None;

        if let Some(ref org) = filter.organization {
            sql.push_str(&format!(" AND organization = ${param_idx}"));
            org_val = Some(org.clone());
            param_idx += 1;
        }
        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(" AND namespace = ${param_idx}"));
            ns_val = Some(ns.clone());
            param_idx += 1;
        }
        if let Some(ref topic) = filter.topic {
            sql.push_str(&format!(" AND topic = ${param_idx}"));
            topic_val = Some(topic.clone());
            param_idx += 1;
        }
        if let Some(ref since) = filter.since {
            sql.push_str(&format!(" AND timestamp >= ${param_idx}"));
            since_val = Some(*since);
        }

        sql.push_str(" ORDER BY timestamp ASC");

        if let Some(limit) = filter.limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        let mut query = sqlx::query(&sql);
        if let Some(ref v) = org_val {
            query = query.bind(v);
        }
        if let Some(ref v) = ns_val {
            query = query.bind(v);
        }
        if let Some(ref v) = topic_val {
            query = query.bind(v);
        }
        if let Some(v) = since_val {
            query = query.bind(v);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results = Vec::with_capacity(rows.len());
        for row in &rows {
            let id: Uuid = row.get("id");
            let organization: String = row.get("organization");
            let namespace: String = row.get("namespace");
            let topic: String = row.get("topic");
            let payload: serde_json::Value = row.get("payload");
            let content_type: String = row.get("content_type");
            let metadata: serde_json::Value = row.get("metadata");
            let timestamp: DateTime<Utc> = row.get("timestamp");
            let version: i64 = row.get("version");

            results.push(SerializedEvent {
                id: id.to_string(),
                organization,
                namespace,
                topic,
                payload,
                content_type,
                metadata: serde_json::from_value(metadata).unwrap_or_default(),
                timestamp,
                version: version as u64,
            });
        }

        Ok(results)
    }
}
