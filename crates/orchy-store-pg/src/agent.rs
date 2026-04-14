use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::PgBackend;

const SELECT_COLS: &str = "id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata";

impl AgentStore for PgBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let roles_json = serde_json::to_value(agent.roles()).unwrap();
        let metadata_json = serde_json::to_value(agent.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO agents (id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
             ON CONFLICT (id) DO UPDATE SET
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                parent_id = EXCLUDED.parent_id,
                roles = EXCLUDED.roles,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                last_heartbeat = EXCLUDED.last_heartbeat,
                connected_at = EXCLUDED.connected_at,
                metadata = EXCLUDED.metadata",
        )
        .bind(agent.id().as_uuid())
        .bind(agent.project().to_string())
        .bind(agent.namespace().to_string())
        .bind(agent.parent_id().map(|id| *id.as_uuid()))
        .bind(&roles_json)
        .bind(agent.description())
        .bind(agent.status().to_string())
        .bind(agent.last_heartbeat())
        .bind(agent.connected_at())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        for evt in &events {
            if let Ok(serialized) = orchy_events::SerializedEvent::from_event(evt) {
                let id = uuid::Uuid::parse_str(&serialized.id).unwrap();
                let _ = sqlx::query(
                    "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
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
                .await;
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_agent(&r)))
    }

    async fn list(&self) -> Result<Vec<Agent>> {
        let sql = format!("SELECT {SELECT_COLS} FROM agents");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_agent).collect())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let sql = format!(
            "SELECT {SELECT_COLS} FROM agents WHERE status != 'disconnected' AND last_heartbeat < $1"
        );
        let rows = sqlx::query(&sql)
            .bind(cutoff)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_agent).collect())
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Agent {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let parent_id: Option<Uuid> = row.get("parent_id");
    let roles: serde_json::Value = row.get("roles");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let last_heartbeat: DateTime<Utc> = row.get("last_heartbeat");
    let connected_at: DateTime<Utc> = row.get("connected_at");
    let metadata: serde_json::Value = row.get("metadata");

    Agent::restore(RestoreAgent {
        id: AgentId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        parent_id: parent_id.map(AgentId::from_uuid),
        roles: serde_json::from_value(roles).unwrap_or_default(),
        description,
        status: status.parse::<AgentStatus>().unwrap_or_default(),
        last_heartbeat,
        connected_at,
        metadata: serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
    })
}
