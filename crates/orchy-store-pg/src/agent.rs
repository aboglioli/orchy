use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::Namespace;

use crate::PgBackend;

impl AgentStore for PgBackend {
    async fn save(&self, agent: &Agent) -> Result<()> {
        let roles_json = serde_json::to_value(agent.roles()).unwrap();
        let metadata_json = serde_json::to_value(agent.metadata()).unwrap();

        sqlx::query(
            "INSERT INTO agents (id, namespace, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (id) DO UPDATE SET
                namespace = EXCLUDED.namespace,
                roles = EXCLUDED.roles,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                last_heartbeat = EXCLUDED.last_heartbeat,
                connected_at = EXCLUDED.connected_at,
                metadata = EXCLUDED.metadata",
        )
        .bind(agent.id().as_uuid())
        .bind(agent.namespace().to_string())
        .bind(&roles_json)
        .bind(agent.description())
        .bind(agent.status().to_string())
        .bind(agent.last_heartbeat())
        .bind(agent.connected_at())
        .bind(&metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let row = sqlx::query(
            "SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata
             FROM agents WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_agent(&r)))
    }

    async fn list(&self) -> Result<Vec<Agent>> {
        let rows = sqlx::query(
            "SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata
             FROM agents",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_agent).collect())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let rows = sqlx::query(
            "SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata
             FROM agents
             WHERE status != 'disconnected' AND last_heartbeat < $1",
        )
        .bind(cutoff)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_agent).collect())
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Agent {
    let id: Uuid = row.get("id");
    let namespace: String = row.get("namespace");
    let roles: serde_json::Value = row.get("roles");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let last_heartbeat: DateTime<Utc> = row.get("last_heartbeat");
    let connected_at: DateTime<Utc> = row.get("connected_at");
    let metadata: serde_json::Value = row.get("metadata");

    Agent::restore(
        AgentId::from_uuid(id),
        Namespace::try_from(namespace).expect("invalid namespace in database"),
        serde_json::from_value(roles).unwrap_or_default(),
        description,
        parse_agent_status(&status),
        last_heartbeat,
        connected_at,
        serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
    )
}

fn parse_agent_status(s: &str) -> AgentStatus {
    match s {
        "online" => AgentStatus::Online,
        "busy" => AgentStatus::Busy,
        "idle" => AgentStatus::Idle,
        "disconnected" => AgentStatus::Disconnected,
        _ => AgentStatus::Online,
    }
}
