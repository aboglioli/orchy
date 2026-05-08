use rusqlite::{Error as RusqliteError, Row as RusqliteRow, types::Type as RusqliteType};
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::iter::repeat_n;
use std::result::Result as StdResult;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result, StoreError};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::{SqliteConn, blocking, blocking_tx, decode_json, events};

const SELECT_COLS: &str = "id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at, version";

pub struct SqliteTaskStore {
    conn: SqliteConn,
}

impl SqliteTaskStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl TaskStore for SqliteTaskStore {
    async fn save(&self, task: &mut Task) -> Result<()> {
        let snapshot = task.clone();
        let drained = task.drain_events();
        blocking_tx(&self.conn, move |tx| {
            if let Some(pv) = snapshot.persisted_version() {
                let affected = tx
                    .execute(
                        "UPDATE tasks SET organization_id=?2, project=?3, namespace=?4, title=?5, \
                         description=?6, acceptance_criteria=?7, status=?8, priority=?9, \
                         assigned_roles=?10, assigned_to=?11, assigned_at=?12, stale_after_secs=?13, \
                         last_activity_at=?14, tags=?15, result_summary=?16, archived_at=?17, \
                         created_by=?18, created_at=?19, updated_at=?20, version=?21 \
                         WHERE id=?1 AND version=?22",
                        rusqlite::params![
                            snapshot.id().to_string(),
                            snapshot.org_id().to_string(),
                            snapshot.project().to_string(),
                            snapshot.namespace().to_string(),
                            snapshot.title(),
                            snapshot.description(),
                            snapshot.acceptance_criteria().map(|s| s.to_string()),
                            snapshot.status().to_string(),
                            snapshot.priority().to_string(),
                            serde_json::to_string(snapshot.assigned_roles()).map_err(|e| {
                                Error::Store(StoreError::Serialization(format!(
                                    "assigned_roles: {e}"
                                )))
                            })?,
                            snapshot.assigned_to().map(|a| a.to_string()),
                            snapshot.assigned_at().map(|dt| dt.to_rfc3339()),
                            snapshot.stale_after_secs(),
                            snapshot.last_activity_at().to_rfc3339(),
                            serde_json::to_string(snapshot.tags()).map_err(|e| {
                                Error::Store(StoreError::Serialization(format!("tags: {e}")))
                            })?,
                            snapshot.result_summary().map(|s| s.to_string()),
                            snapshot.archived_at().map(|dt| dt.to_rfc3339()),
                            snapshot.created_by().map(|a| a.to_string()),
                            snapshot.created_at().to_rfc3339(),
                            snapshot.updated_at().to_rfc3339(),
                            snapshot.version(),
                            pv,
                        ],
                    )
                    .map_err(crate::error::store_err)?;

                if affected == 0 {
                    return Err(Error::version_mismatch(pv, snapshot.version()));
                }
            } else {
                tx.execute(
                    "INSERT OR REPLACE INTO tasks (id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at, version)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
                    rusqlite::params![
                        snapshot.id().to_string(),
                        snapshot.org_id().to_string(),
                        snapshot.project().to_string(),
                        snapshot.namespace().to_string(),
                        snapshot.title(),
                        snapshot.description(),
                        snapshot.acceptance_criteria().map(|s| s.to_string()),
                        snapshot.status().to_string(),
                        snapshot.priority().to_string(),
                        serde_json::to_string(snapshot.assigned_roles()).map_err(|e| {
                            Error::Store(StoreError::Serialization(format!("assigned_roles: {e}")))
                        })?,
                        snapshot.assigned_to().map(|a| a.to_string()),
                        snapshot.assigned_at().map(|dt| dt.to_rfc3339()),
                        snapshot.stale_after_secs(),
                        snapshot.last_activity_at().to_rfc3339(),
                        serde_json::to_string(snapshot.tags()).map_err(|e| {
                            Error::Store(StoreError::Serialization(format!("tags: {e}")))
                        })?,
                        snapshot.result_summary().map(|s| s.to_string()),
                        snapshot.archived_at().map(|dt| dt.to_rfc3339()),
                        snapshot.created_by().map(|a| a.to_string()),
                        snapshot.created_at().to_rfc3339(),
                        snapshot.updated_at().to_rfc3339(),
                        snapshot.version(),
                    ],
                )
                .map_err(crate::error::store_err)?;
            }

            events::write_events_in_tx(tx, &drained)?;
            Ok(())
        })
        .await?;

        task.mark_persisted();
        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let id = id.to_string();
        blocking(&self.conn, move |conn| {
            let sql = format!("SELECT {SELECT_COLS} FROM tasks WHERE id = ?1");
            conn.query_row(&sql, rusqlite::params![id], row_to_task)
                .optional()
                .map_err(crate::error::store_err)
        })
        .await
    }

    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let id_strings: Vec<String> = ids.iter().map(|id| id.to_string()).collect();
        blocking(&self.conn, move |conn| {
            let placeholders: String = repeat_n("?", id_strings.len())
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!("SELECT {SELECT_COLS} FROM tasks WHERE id IN ({placeholders})");
            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
            let param_refs: Vec<&dyn rusqlite::ToSql> = id_strings
                .iter()
                .map(|s| s as &dyn rusqlite::ToSql)
                .collect();
            stmt.query_map(param_refs.as_slice(), row_to_task)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)
        })
        .await
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        blocking(&self.conn, move |conn| {
            let mut sql = format!("SELECT {SELECT_COLS} FROM tasks WHERE 1=1");
            let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(ref org_id) = filter.org_id {
                sql.push_str(&format!(" AND organization_id = ?{idx}"));
                params.push(Box::new(org_id.to_string()));
                idx += 1;
            }
            if let Some(ref ns) = filter.namespace {
                if !ns.is_root() {
                    sql.push_str(&format!(
                        " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
                    ));
                    params.push(Box::new(ns.to_string()));
                    idx += 1;
                }
            }
            if let Some(ref project) = filter.project {
                sql.push_str(&format!(" AND project = ?{idx}"));
                params.push(Box::new(project.to_string()));
                idx += 1;
            }
            if let Some(ref status) = filter.status {
                sql.push_str(&format!(" AND status = ?{idx}"));
                params.push(Box::new(status.to_string()));
                idx += 1;
            }
            if let Some(ref role) = filter.assigned_role {
                sql.push_str(&format!(
                    " AND (assigned_roles = '[]' OR EXISTS (SELECT 1 FROM json_each(assigned_roles) WHERE value = ?{idx}))"
                ));
                params.push(Box::new(role.to_string()));
                idx += 1;
            }
            if let Some(ref assigned) = filter.assigned_to {
                sql.push_str(&format!(" AND assigned_to = ?{idx}"));
                params.push(Box::new(assigned.to_string()));
                idx += 1;
            }
            if let Some(ref tag) = filter.tag {
                sql.push_str(&format!(
                    " AND EXISTS (SELECT 1 FROM json_each(tags) WHERE value = ?{idx})"
                ));
                params.push(Box::new(tag.to_string()));
                idx += 1;
            }
            if !filter.include_archived.unwrap_or(false) {
                sql.push_str(" AND archived_at IS NULL");
            }

            if let Some(ref cursor) = page.after {
                if let Some(decoded) = decode_cursor(cursor) {
                    sql.push_str(&format!(" AND id < ?{idx}"));
                    params.push(Box::new(decoded));
                    idx += 1;
                }
            }

            let _ = idx;
            sql.push_str(" ORDER BY CASE priority WHEN 'critical' THEN 3 WHEN 'high' THEN 2 WHEN 'normal' THEN 1 WHEN 'low' THEN 0 ELSE 1 END DESC, id DESC");

            let fetch_limit = (page.limit as u64).saturating_add(1);
            sql.push_str(&format!(" LIMIT {fetch_limit}"));

            let mut stmt = conn.prepare(&sql).map_err(crate::error::store_err)?;
            let param_refs: Vec<&dyn rusqlite::types::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let mut tasks: Vec<Task> = stmt
                .query_map(param_refs.as_slice(), row_to_task)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)?;

            let has_more = tasks.len() > page.limit as usize;
            if has_more {
                tasks.truncate(page.limit as usize);
            }

            let next_cursor = if has_more {
                tasks.last().map(|t| encode_cursor(&t.id().to_string()))
            } else {
                None
            };

            Ok(Page::new(tasks, next_cursor))
        })
        .await
    }
}

fn row_to_task(row: &RusqliteRow) -> rusqlite::Result<Task> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let project_str: String = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let title: String = row.get(4)?;
    let description: String = row.get(5)?;
    let acceptance_criteria: Option<String> = row.get(6)?;
    let status_str: String = row.get(7)?;
    let priority_str: String = row.get(8)?;
    let roles_str: String = row.get(9)?;
    let assigned_to_str: Option<String> = row.get(10)?;
    let assigned_at_str: Option<String> = row.get(11)?;
    let stale_after_secs: Option<u64> = row.get(12)?;
    let last_activity_at_str: String = row.get(13)?;
    let tags_str: String = row.get(14)?;
    let result_summary: Option<String> = row.get(15)?;
    let archived_at_str: Option<String> = row.get(16)?;
    let created_by_str: Option<String> = row.get(17)?;
    let created_at_str: String = row.get(18)?;
    let updated_at_str: String = row.get(19)?;
    let version: u64 = row.get(20)?;

    let tags: Vec<String> = decode_json(&tags_str, "tags")?;

    let id = TaskId::from_str(&id_str)
        .map_err(|e| RusqliteError::FromSqlConversionFailure(0, RusqliteType::Text, Box::new(e)))?;
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        RusqliteError::FromSqlConversionFailure(
            1,
            RusqliteType::Text,
            Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string())),
        )
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        RusqliteError::FromSqlConversionFailure(
            2,
            RusqliteType::Text,
            Box::new(IoError::new(IoErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        RusqliteError::FromSqlConversionFailure(
            3,
            RusqliteType::Text,
            Box::new(IoError::new(IoErrorKind::InvalidData, e)),
        )
    })?;
    let status = status_str
        .parse::<TaskStatus>()
        .unwrap_or(TaskStatus::Pending);
    let priority = priority_str.parse::<Priority>().unwrap_or_default();
    let assigned_roles: Vec<String> = decode_json(&roles_str, "assigned_roles")?;
    let assigned_to = assigned_to_str.and_then(|s| AgentId::from_str(&s).ok());
    let assigned_at = assigned_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let last_activity_at = DateTime::parse_from_rfc3339(&last_activity_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            RusqliteError::FromSqlConversionFailure(13, RusqliteType::Text, Box::new(e))
        })?;
    let archived_at = archived_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let created_by = created_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            RusqliteError::FromSqlConversionFailure(18, RusqliteType::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            RusqliteError::FromSqlConversionFailure(19, RusqliteType::Text, Box::new(e))
        })?;

    Ok(Task::restore(RestoreTask {
        id,
        org_id,
        project,
        namespace,
        title,
        description,
        acceptance_criteria,
        status,
        priority,
        assigned_roles,
        assigned_to,
        assigned_at,
        stale_after_secs,
        last_activity_at,
        tags,
        result_summary,
        archived_at,
        created_by,
        created_at,
        updated_at,
        version,
    }))
}
