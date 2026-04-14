use std::collections::HashMap;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::SqliteBackend;

const SELECT_COLS: &str = "id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata";

impl AgentStore for SqliteBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO agents (id, project, namespace, parent_id, roles, description, status, last_heartbeat, connected_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                agent.id().to_string(),
                agent.project().to_string(),
                agent.namespace().to_string(),
                agent.parent_id().map(|id| id.to_string()),
                serde_json::to_string(agent.roles()).unwrap(),
                agent.description(),
                agent.status().to_string(),
                agent.last_heartbeat().to_rfc3339(),
                agent.connected_at().to_rfc3339(),
                serde_json::to_string(agent.metadata()).unwrap(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        for evt in &events {
            if let Ok(serialized) = orchy_events::SerializedEvent::from_event(evt) {
                let _ = conn.execute(
                    "INSERT INTO events (id, organization, namespace, topic, payload, content_type, metadata, timestamp, version) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        serialized.id,
                        serialized.organization,
                        serialized.namespace,
                        serialized.topic,
                        serde_json::to_string(&serialized.payload).unwrap(),
                        serialized.content_type,
                        serde_json::to_string(&serialized.metadata).unwrap(),
                        serialized.timestamp.to_rfc3339(),
                        serialized.version,
                    ],
                );
            }
        }

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

    async fn list(&self) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let sql = format!("SELECT {SELECT_COLS} FROM agents");
        let mut stmt = conn
            .prepare(&sql)
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
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let parent_id_str: Option<String> = row.get(3)?;
    let roles_str: String = row.get(4)?;
    let description: String = row.get(5)?;
    let status_str: String = row.get(6)?;
    let heartbeat_str: String = row.get(7)?;
    let connected_str: String = row.get(8)?;
    let metadata_str: String = row.get(9)?;

    let parent_id = parent_id_str
        .map(|s| AgentId::from_str(&s))
        .transpose()
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        project: ProjectId::try_from(project_str).map_err(|e| conversion_err(1, e))?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| conversion_err(2, e))?,
        parent_id,
        roles: serde_json::from_str(&roles_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        description,
        status: status_str.parse::<AgentStatus>().unwrap_or_default(),
        last_heartbeat: DateTime::parse_from_rfc3339(&heartbeat_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        connected_at: DateTime::parse_from_rfc3339(&connected_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    8,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        metadata: serde_json::from_str(&metadata_str).unwrap_or_else(|_| HashMap::new()),
    }))
}
