use rusqlite::{Error as RusqliteError, Row as RusqliteRow, types::Type as RusqliteType};
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::iter::repeat_n;
use std::result::Result as StdResult;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::{Agent, AgentId, AgentStore, Alias, RestoreAgent};
use orchy_core::error::{Error, Result, StoreError};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::user::UserId;

use crate::{SqliteConn, decode_json, events};

const SELECT_COLS: &str = "id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata, user_id";

pub struct SqliteAgentStore {
    conn: SqliteConn,
}

impl SqliteAgentStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl AgentStore for SqliteAgentStore {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let mut conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let tx = conn.transaction().map_err(crate::error::store_err)?;

        tx.execute(
            "INSERT INTO agents (id, alias, organization_id, project, namespace, roles, description, last_seen, connected_at, metadata, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
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
            rusqlite::params![
                agent.id().to_string(),
                agent.alias().as_str(),
                agent.org_id().to_string(),
                agent.project().to_string(),
                agent.namespace().to_string(),
                serde_json::to_string(agent.roles())
                    .map_err(|e| Error::Store(StoreError::Serialization(format!("roles: {e}"))))?,
                agent.description(),
                agent.last_seen().to_rfc3339(),
                agent.connected_at().to_rfc3339(),
                serde_json::to_string(agent.metadata())
                    .map_err(|e| Error::Store(StoreError::Serialization(format!("metadata: {e}"))))?,
                agent.user_id().map(|u| u.to_string()),
            ],
        )
        .map_err(crate::error::store_err)?;

        let events = agent.drain_events();
        events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id = ?1");
        let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_agent)
            .optional()
            .map_err(crate::error::store_err)?;

        Ok(result)
    }

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let sql = format!(
            "SELECT {SELECT_COLS} FROM agents WHERE organization_id = ?1 AND project = ?2 AND alias = ?3"
        );
        let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
        stmt.query_row(
            rusqlite::params![org.to_string(), project.to_string(), alias.as_str()],
            row_to_agent,
        )
        .optional()
        .map_err(crate::error::store_err)
    }

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

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

        let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut agents: Vec<Agent> = stmt
            .query_map(param_refs.as_slice(), row_to_agent)
            .map_err(crate::error::store_err)?
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(crate::error::store_err)?;

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
        let placeholders: String = repeat_n("?", ids.len()).collect::<Vec<_>>().join(", ");
        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE id IN ({placeholders})");
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> = id_strings
            .iter()
            .map(|s| s as &dyn rusqlite::ToSql)
            .collect();
        let agents = stmt
            .query_map(param_refs.as_slice(), row_to_agent)
            .map_err(crate::error::store_err)?
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(crate::error::store_err)?;
        Ok(agents)
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let cutoff = Utc::now() - Duration::seconds(timeout_secs as i64);

        let sql = format!("SELECT {SELECT_COLS} FROM agents WHERE last_seen < ?1");
        let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;

        let agents = stmt
            .query_map(rusqlite::params![cutoff.to_rfc3339()], row_to_agent)
            .map_err(crate::error::store_err)?
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(crate::error::store_err)?;

        Ok(agents)
    }
}

fn conversion_err(col: usize, msg: impl Into<String>) -> RusqliteError {
    RusqliteError::FromSqlConversionFailure(
        col,
        RusqliteType::Text,
        Box::new(IoError::new(IoErrorKind::InvalidData, msg.into())),
    )
}

fn row_to_agent(row: &RusqliteRow) -> rusqlite::Result<Agent> {
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
    let user_id_str: Option<String> = row.get(10).ok();

    let user_id = user_id_str.and_then(|s| UserId::from_str(&s).ok());

    Ok(Agent::restore(RestoreAgent {
        id: AgentId::from_str(&id_str).map_err(|e| conversion_err(0, e.to_string()))?,
        alias: Alias::from_string_unchecked(alias),
        org_id: OrganizationId::new(&org_id_str).map_err(|e| conversion_err(2, e.to_string()))?,
        project: ProjectId::try_from(project_str).map_err(|e| conversion_err(3, e.to_string()))?,
        namespace: Namespace::try_from(namespace_str)
            .map_err(|e| conversion_err(4, e.to_string()))?,
        roles: decode_json(&roles_str, "roles")?,
        description,
        last_seen: DateTime::parse_from_rfc3339(&last_seen_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                RusqliteError::FromSqlConversionFailure(7, RusqliteType::Text, Box::new(e))
            })?,
        connected_at: DateTime::parse_from_rfc3339(&connected_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                RusqliteError::FromSqlConversionFailure(8, RusqliteType::Text, Box::new(e))
            })?,
        metadata: decode_json(&metadata_str, "metadata")?,
        user_id,
    }))
}
