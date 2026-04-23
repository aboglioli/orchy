use std::collections::HashMap;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;

use orchy_core::agent::{Agent, AgentId, AgentStore, Alias, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::user::UserId;
use orchy_events::io::Writer;

use crate::{
    PgBackend, decode_json_value, events::PgEventWriter, parse_namespace, parse_project_id,
};

const SELECT_COLS: &str = "id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata, user_id";

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
            "INSERT INTO agents (id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata, user_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (id) DO UPDATE SET
                alias = EXCLUDED.alias,
                organization_id = EXCLUDED.organization_id,
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                roles = EXCLUDED.roles,
                description = EXCLUDED.description,
                last_seen = EXCLUDED.last_seen,
                connected_at = EXCLUDED.connected_at,
                metadata = EXCLUDED.metadata,
                user_id = EXCLUDED.user_id",
        )
        .bind(agent.id().to_string())
        .bind(agent.alias().as_str())
        .bind(agent.org_id().to_string())
        .bind(agent.project().to_string())
        .bind(agent.namespace().to_string())
        .bind(&roles_json)
        .bind(agent.description())
        .bind(agent.last_seen())
        .bind(agent.connected_at())
        .bind(&metadata_json)
        .bind(agent.user_id().map(|u| u.to_string()))
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        PgEventWriter::new_tx(&mut tx)
            .write_all(&events)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

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

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        let sql = format!(
            "SELECT {SELECT_COLS} FROM agents WHERE organization_id = $1 AND project = $2 AND alias = $3"
        );
        let row = sqlx::query(&sql)
            .bind(org.to_string())
            .bind(project.to_string())
            .bind(alias.as_str())
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

    async fn find_by_ids(&self, ids: &[AgentId]) -> Result<Vec<Agent>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let str_ids: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id = ANY($1::text[])");
        let rows = sqlx::query(&sql)
            .bind(&str_ids)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_agent).collect()
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE last_seen < $1");
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
    let alias: String = row.get("alias");
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let roles: serde_json::Value = row.get("roles");
    let description: String = row.get("description");
    let last_seen: DateTime<Utc> = row.get("last_seen");
    let connected_at: DateTime<Utc> = row.get("connected_at");
    let metadata: serde_json::Value = row.get("metadata");
    let user_id_str: Option<String> = row.try_get("user_id").ok();

    let user_id = user_id_str.and_then(|s| UserId::from_str(&s).ok());

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str)?,
        alias: Alias::new(&alias).unwrap_or_else(|_| {
            Alias::new(&format!("agent-{id_str}"))
                .unwrap_or_else(|_| Alias::new("unknown").unwrap())
        }),
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid agents.organization_id: {e}")))?,
        project: parse_project_id(project, "agents", "project")?,
        namespace: parse_namespace(namespace, "agents", "namespace")?,
        roles: decode_json_value(roles, "agents", "roles")?,
        description,
        last_seen,
        connected_at,
        metadata: decode_json_value::<HashMap<String, String>>(metadata, "agents", "metadata")?,
        user_id,
    }))
}
