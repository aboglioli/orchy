use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::resource_ref::ResourceRef;
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::SqliteBackend;

#[async_trait]
impl TaskStore for SqliteBackend {
    async fn save(&self, task: &mut Task) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO tasks (id, organization_id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, tags, result_summary, refs, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            rusqlite::params![
                task.id().to_string(),
                task.org_id().to_string(),
                task.project().to_string(),
                task.namespace().to_string(),
                task.parent_id().map(|id| id.to_string()),
                task.title(),
                task.description(),
                task.status().to_string(),
                task.priority().to_string(),
                serde_json::to_string(task.assigned_roles())
                    .map_err(|e| Error::Store(format!("failed to serialize assigned_roles: {e}")))?,
                task.assigned_to().map(|a| a.to_string()),
                task.assigned_at().map(|dt| dt.to_rfc3339()),
                serde_json::to_string(&task.depends_on().iter().map(|t| t.to_string()).collect::<Vec<_>>())
                    .map_err(|e| Error::Store(format!("failed to serialize depends_on: {e}")))?,
                serde_json::to_string(task.tags())
                    .map_err(|e| Error::Store(format!("failed to serialize tags: {e}")))?,
                task.result_summary().map(|s| s.to_string()),
                serde_json::to_string(task.refs())
                    .map_err(|e| Error::Store(format!("failed to serialize refs: {e}")))?,
                task.created_by().map(|a| a.to_string()),
                task.created_at().to_rfc3339(),
                task.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = task.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, tags, result_summary, refs, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_task)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, organization_id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, tags, result_summary, refs, created_by, created_at, updated_at FROM tasks WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

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
                " AND (assigned_roles = '[]' OR assigned_roles LIKE ?{idx})"
            ));
            params.push(Box::new(format!("%{role}%")));
            idx += 1;
        }
        if let Some(ref assigned) = filter.assigned_to {
            sql.push_str(&format!(" AND assigned_to = ?{idx}"));
            params.push(Box::new(assigned.to_string()));
            idx += 1;
        }
        if let Some(ref pid) = filter.parent_id {
            sql.push_str(&format!(" AND parent_id = ?{idx}"));
            params.push(Box::new(pid.to_string()));
            idx += 1;
        }
        if let Some(ref tag) = filter.tag {
            sql.push_str(&format!(" AND tags LIKE ?{idx}"));
            params.push(Box::new(format!("%{tag}%")));
            idx += 1;
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

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut tasks: Vec<Task> = stmt
            .query_map(param_refs.as_slice(), row_to_task)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

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
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    let id_str: String = row.get(0)?;
    let org_id_str: String = row.get(1)?;
    let project_str: String = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let parent_id_str: Option<String> = row.get(4)?;
    let title: String = row.get(5)?;
    let description: String = row.get(6)?;
    let status_str: String = row.get(7)?;
    let priority_str: String = row.get(8)?;
    let roles_str: String = row.get(9)?;
    let assigned_to_str: Option<String> = row.get(10)?;
    let assigned_at_str: Option<String> = row.get(11)?;
    let depends_on_str: String = row.get(12)?;
    let tags_str: String = row.get(13)?;
    let result_summary: Option<String> = row.get(14)?;
    let refs_str: String = row.get(15)?;
    let created_by_str: Option<String> = row.get(16)?;
    let created_at_str: String = row.get(17)?;
    let updated_at_str: String = row.get(18)?;

    let depends_on_strs: Vec<String> = crate::decode_json(&depends_on_str, "depends_on")?;
    let depends_on: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| TaskId::from_str(s).ok())
        .collect();

    let tags: Vec<String> = crate::decode_json(&tags_str, "tags")?;
    let refs: Vec<ResourceRef> = crate::decode_json(&refs_str, "refs")?;

    let id = TaskId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e.to_string(),
            )),
        )
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let status = status_str
        .parse::<TaskStatus>()
        .unwrap_or(TaskStatus::Pending);
    let priority = priority_str.parse::<Priority>().unwrap_or_default();
    let assigned_roles: Vec<String> = crate::decode_json(&roles_str, "assigned_roles")?;
    let parent_id = parent_id_str.and_then(|s| TaskId::from_str(&s).ok());
    let assigned_to = assigned_to_str.and_then(|s| AgentId::from_str(&s).ok());
    let assigned_at = assigned_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let created_by = created_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(18, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Task::restore(RestoreTask {
        id,
        org_id,
        project,
        namespace,
        parent_id,
        title,
        description,
        status,
        priority,
        assigned_roles,
        assigned_to,
        assigned_at,
        depends_on,
        tags,
        result_summary,
        refs,
        created_by,
        created_at,
        updated_at,
    }))
}
