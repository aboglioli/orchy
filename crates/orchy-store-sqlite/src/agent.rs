use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;

use crate::SqliteBackend;

const SELECT_COLS: &str = "id, organization_id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata";

#[async_trait]
impl AgentStore for SqliteBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO agents (id, organization_id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                agent.id().to_string(),
                agent.org_id().to_string(),
                agent.project().to_string(),
                agent.namespace().to_string(),
                agent.parent_id().map(|id| id.to_string()),
                serde_json::to_string(agent.roles())
                    .map_err(|e| Error::Store(format!("failed to serialize roles: {e}")))?,
                agent.description(),
                agent.status().to_string(),
                agent.last_heartbeat().to_rfc3339(),
                agent.connected_at().to_rfc3339(),
                serde_json::to_string(agent.metadata())
                    .map_err(|e| Error::Store(format!("failed to serialize metadata: {e}")))?,
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id = ?1");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_agent)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, org: &OrganizationId) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE organization_id = ?1");
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;

        let agents = stmt
            .query_map(rusqlite::params![org.to_string()], row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agents)
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let sql = format!(
            "SELECT {SELECT_COLS} FROM agents WHERE status != 'disconnected' AND last_heartbeat < ?1"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;

        let agents = stmt
            .query_map(rusqlite::params![cutoff.to_rfc3339()], row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agents)
    }
}

fn conversion_err(col: usize, msg: impl Into<String>) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        col,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            msg.into(),
        )),
    )
}

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<Agent> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let project_str: String = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let parent_id_str: Option<String> = row.get(4)?;
    let roles_str: String = row.get(5)?;
    let description: String = row.get(6)?;
    let status_str: String = row.get(7)?;
    let heartbeat_str: String = row.get(8)?;
    let connected_str: String = row.get(9)?;
    let metadata_str: String = row.get(10)?;

    let parent_id = parent_id_str
        .map(|s| AgentId::from_str(&s))
        .transpose()
        .map_err(|e| conversion_err(4, e))?;

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str).map_err(|e| conversion_err(0, e))?,
        org_id: OrganizationId::new(&org_id_str).map_err(|e| conversion_err(1, e.to_string()))?,
        project: ProjectId::try_from(project_str).map_err(|e| conversion_err(2, e))?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| conversion_err(3, e))?,
        parent_id,
        roles: crate::decode_json(&roles_str, "roles")?,
        description,
        status: status_str.parse::<AgentStatus>().unwrap_or_default(),
        last_heartbeat: DateTime::parse_from_rfc3339(&heartbeat_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        connected_at: DateTime::parse_from_rfc3339(&connected_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        metadata: crate::decode_json(&metadata_str, "metadata")?,
    }))
}
