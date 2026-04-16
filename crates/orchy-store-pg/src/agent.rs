use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;

use crate::{PgBackend, decode_json_value, parse_namespace, parse_project_id};

const SELECT_COLS: &str = "id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata";

impl AgentStore for PgBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let roles_json = serde_json::to_value(agent.roles())
            .map_err(|e| Error::Store(format!("failed to serialize agents.roles: {e}")))?;
        let metadata_json = serde_json::to_value(agent.metadata())
            .map_err(|e| Error::Store(format!("failed to serialize agents.metadata: {e}")))?;

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
        .bind(agent.id().to_string())
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
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id = $1");
        let row = sqlx::query(&sql)
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_agent(&r)).transpose()
    }

    async fn list(&self, _org: &OrganizationId) -> Result<Vec<Agent>> {
        let sql = format!("SELECT {SELECT_COLS} FROM agents");
        let rows = sqlx::query(&sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_agent).collect()
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

        rows.iter().map(row_to_agent).collect()
    }
}

fn row_to_agent(row: &sqlx::postgres::PgRow) -> Result<Agent> {
    let id_str: String = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let parent_id_str: Option<String> = row.get("parent_id");
    let roles: serde_json::Value = row.get("roles");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let last_heartbeat: DateTime<Utc> = row.get("last_heartbeat");
    let connected_at: DateTime<Utc> = row.get("connected_at");
    let metadata: serde_json::Value = row.get("metadata");

    let parent_id = parent_id_str
        .map(|s| AgentId::from_str(&s).map_err(Error::Store))
        .transpose()?;

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str).map_err(Error::Store)?,
        org_id: OrganizationId::new("default").unwrap(),
        project: parse_project_id(project, "agents", "project")?,
        namespace: parse_namespace(namespace, "agents", "namespace")?,
        parent_id,
        roles: decode_json_value(roles, "agents", "roles")?,
        description,
        status: status.parse::<AgentStatus>().unwrap_or_default(),
        last_heartbeat,
        connected_at,
        metadata: decode_json_value::<HashMap<String, String>>(metadata, "agents", "metadata")?,
    }))
}
