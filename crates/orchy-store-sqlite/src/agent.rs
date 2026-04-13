use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::Namespace;

use crate::SqliteBackend;

impl AgentStore for SqliteBackend {
    async fn save(&self, agent: &Agent) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO agents (id, namespace, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                agent.id().to_string(),
                agent.namespace().to_string(),
                serde_json::to_string(agent.roles()).unwrap(),
                agent.description(),
                agent.status().to_string(),
                agent.last_heartbeat().to_rfc3339(),
                agent.connected_at().to_rfc3339(),
                serde_json::to_string(agent.metadata()).unwrap(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata FROM agents WHERE id = ?1")
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_agent)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare("SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata FROM agents")
            .map_err(|e| Error::Store(e.to_string()))?;

        let agents = stmt
            .query_map([], row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agents)
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, roles, description, status, last_heartbeat, connected_at, metadata
                 FROM agents
                 WHERE status != 'disconnected' AND last_heartbeat < ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let agents = stmt
            .query_map(rusqlite::params![cutoff.to_rfc3339()], row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agents)
    }
}

fn row_to_agent(row: &rusqlite::Row) -> rusqlite::Result<Agent> {
    let id_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let roles_str: String = row.get(2)?;
    let description: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let heartbeat_str: String = row.get(5)?;
    let connected_str: String = row.get(6)?;
    let metadata_str: String = row.get(7)?;

    Ok(Agent::restore(
        AgentId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        serde_json::from_str(&roles_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
        })?,
        description,
        parse_agent_status(&status_str),
        DateTime::parse_from_rfc3339(&heartbeat_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        DateTime::parse_from_rfc3339(&connected_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        serde_json::from_str(&metadata_str).unwrap_or_else(|_| HashMap::new()),
    ))
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
