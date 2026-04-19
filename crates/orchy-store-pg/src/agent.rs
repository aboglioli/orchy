use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};

use crate::{PgBackend, decode_json_value, parse_namespace, parse_project_id};

const SELECT_COLS: &str = "id, organization_id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata";

#[async_trait]
impl AgentStore for PgBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let roles_json = serde_json::to_value(agent.roles())
            .map_err(|e| Error::Store(format!("failed to serialize agents.roles: {e}")))?;
        let metadata_json = serde_json::to_value(agent.metadata())
            .map_err(|e| Error::Store(format!("failed to serialize agents.metadata: {e}")))?;

        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO agents (id, organization_id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (id) DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
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
        .bind(agent.org_id().to_string())
        .bind(agent.project().to_string())
        .bind(agent.namespace().to_string())
        .bind(agent.parent_id().map(|id| *id.as_uuid()))
        .bind(&roles_json)
        .bind(agent.description())
        .bind(agent.status().to_string())
        .bind(agent.last_heartbeat())
        .bind(agent.connected_at())
        .bind(&metadata_json)
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
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

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let mut sql = format!("SELECT {SELECT_COLS} FROM agents WHERE organization_id = $1");
        let mut param_idx = 2u32;

        if let Some(ref cursor) = page.after
            && let Some(decoded) = decode_cursor(cursor)
        {
            sql.push_str(&format!(" AND id < ${param_idx}"));
            param_idx += 1;
            let _ = param_idx;

            sql.push_str(" ORDER BY id DESC");
            let fetch_limit = (page.limit as u64).saturating_add(1);
            sql.push_str(&format!(" LIMIT {fetch_limit}"));

            let mut agents: Vec<Agent> = sqlx::query(&sql)
                .bind(org.to_string())
                .bind(decoded)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| Error::Store(e.to_string()))?
                .iter()
                .map(row_to_agent)
                .collect::<Result<Vec<_>>>()?;

            let has_more = agents.len() > page.limit as usize;
            if has_more {
                agents.truncate(page.limit as usize);
            }
            let next_cursor = if has_more {
                agents.last().map(|a| encode_cursor(&a.id().to_string()))
            } else {
                None
            };

            return Ok(Page::new(agents, next_cursor));
        }

        sql.push_str(" ORDER BY id DESC");
        let fetch_limit = (page.limit as u64).saturating_add(1);
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut agents: Vec<Agent> = sqlx::query(&sql)
            .bind(org.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
            .iter()
            .map(row_to_agent)
            .collect::<Result<Vec<_>>>()?;

        let has_more = agents.len() > page.limit as usize;
        if has_more {
            agents.truncate(page.limit as usize);
        }
        let next_cursor = if has_more {
            agents.last().map(|a| encode_cursor(&a.id().to_string()))
        } else {
            None
        };

        Ok(Page::new(agents, next_cursor))
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
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let parent_id_str: Option<String> = row.get("parent_id");
    let roles: serde_json::Value = row.get("roles");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let last_heartbeat: DateTime<Utc> = row.get("last_heartbeat");
    let connected_at: DateTime<Utc> = row.get("connected_at");
    let metadata: serde_json::Value = row.get("metadata");

    let parent_id = parent_id_str.map(|s| AgentId::from_str(&s)).transpose()?;

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str)?,
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid agents.organization_id: {e}")))?,
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
