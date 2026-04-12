use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::entities::{Agent, RegisterAgent};
use orchy_core::error::{Error, Result};
use orchy_core::store::AgentStore;
use orchy_core::value_objects::{AgentId, AgentStatus, Namespace};

use crate::PgBackend;

impl AgentStore for PgBackend {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        let now = Utc::now();
        let agent = Agent {
            id: AgentId::new(),
            namespace: registration.namespace,
            roles: registration.roles,
            description: registration.description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata: registration.metadata,
        };

        let roles_json = serde_json::to_value(&agent.roles).unwrap();
        let metadata_json = serde_json::to_value(&agent.metadata).unwrap();

        sqlx::query(
            "INSERT INTO agents (id, namespace, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(agent.id.as_uuid())
        .bind(agent.namespace.to_string())
        .bind(&roles_json)
        .bind(&agent.description)
        .bind(agent.status.to_string())
        .bind(agent.last_heartbeat)
        .bind(agent.connected_at)
        .bind(&metadata_json)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agent)
    }

    async fn get(&self, id: &AgentId) -> Result<Option<Agent>> {
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

    async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        let result =
            sqlx::query("UPDATE agents SET last_heartbeat = $1, status = 'online' WHERE id = $2")
                .bind(Utc::now())
                .bind(id.as_uuid())
                .execute(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("agent {id}")));
        }
        Ok(())
    }

    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        let result = sqlx::query("UPDATE agents SET status = $1 WHERE id = $2")
            .bind(status.to_string())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("agent {id}")));
        }
        Ok(())
    }

    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        self.update_status(id, AgentStatus::Disconnected).await
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

    Agent {
        id: AgentId::from_uuid(id),
        namespace: Namespace::try_from(namespace).expect("invalid namespace in database"),
        roles: serde_json::from_value(roles).unwrap_or_default(),
        description,
        status: parse_agent_status(&status),
        last_heartbeat,
        connected_at,
        metadata: serde_json::from_value(metadata).unwrap_or_else(|_| HashMap::new()),
    }
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
