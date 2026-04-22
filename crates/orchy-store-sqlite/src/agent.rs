use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::{Agent, AgentId, AgentStore, Alias, RestoreAgent};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};

use crate::SqliteBackend;

const SELECT_COLS: &str = "id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata";

#[async_trait]
impl AgentStore for SqliteBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO agents (id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            rusqlite::params![
                agent.id().to_string(),
                agent.alias().as_str(),
                agent.org_id().to_string(),
                agent.project().to_string(),
                agent.namespace().to_string(),
                serde_json::to_string(agent.roles())
                    .map_err(|e| Error::Store(format!("failed to serialize roles: {e}")))?,
                agent.description(),
                agent.last_seen().to_rfc3339(),
                agent.connected_at().to_rfc3339(),
                serde_json::to_string(agent.metadata())
                    .map_err(|e| Error::Store(format!("failed to serialize metadata: {e}")))?,
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = agent.drain_events();
        crate::events::write_events_in_tx(&tx, &events)?;

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

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let sql = format!(
            "SELECT {SELECT_COLS} FROM agents WHERE organization_id = ?1 AND project = ?2 AND alias = ?3"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        stmt.query_row(
            rusqlite::params![org.to_string(), project.to_string(), alias.as_str()],
            row_to_agent,
        )
        .optional()
        .map_err(|e| Error::Store(e.to_string()))
    }

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = format!("SELECT {SELECT_COLS} FROM agents WHERE organization_id = ?1");
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(org.to_string())];
        let mut idx = 2;

        if let Some(ref cursor) = page.after {
            if let Some(decoded) = decode_cursor(cursor) {
                sql.push_str(&format!(" AND id < ?{idx}"));
                params.push(Box::new(decoded));
                idx += 1;
            }
        }

        let _ = idx;
        sql.push_str(" ORDER BY id DESC");

        let fetch_limit = (page.limit as u64).saturating_add(1);
        sql.push_str(&format!(" LIMIT {fetch_limit}"));

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut agents: Vec<Agent> = stmt
            .query_map(param_refs.as_slice(), row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

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
        let placeholders: String = std::iter::repeat_n("?", ids.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id IN ({placeholders})");
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> = id_strings
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        let agents = stmt
            .query_map(param_refs.as_slice(), row_to_agent)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(agents)
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let cutoff = Utc::now() - chrono::Duration::seconds(timeout_secs as i64);

        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE last_seen < ?1");
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
    let alias: String = row.get(1)?;
    let org_id_str: String = row.get(2)?;
    let project_str: String = row.get(3)?;
    let namespace_str: String = row.get(4)?;
    let roles_str: String = row.get(5)?;
    let description: String = row.get(6)?;
    let last_seen_str: String = row.get(7)?;
    let connected_str: String = row.get(8)?;
    let metadata_str: String = row.get(9)?;

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str).map_err(|e| conversion_err(0, e.to_string()))?,
        alias: Alias::new(&alias).unwrap_or_else(|_| Alias::new(&format!("agent-{id_str}")).unwrap_or_else(|_| Alias::new("unknown").unwrap())),
        org_id: OrganizationId::new(&org_id_str).map_err(|e| conversion_err(2, e.to_string()))?,
        project: ProjectId::try_from(project_str).map_err(|e| conversion_err(3, e))?,
        namespace: Namespace::try_from(namespace_str)
            .map_err(|e| conversion_err(4, e.to_string()))?,
        roles: crate::decode_json(&roles_str, "roles")?,
        description,
        last_seen: DateTime::parse_from_rfc3339(&last_seen_str)
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
        metadata: crate::decode_json(&metadata_str, "metadata")?,
    }))
}
