use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::Namespace;
use orchy_core::note::Note;
use orchy_core::task::{Priority, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::SqliteBackend;

impl TaskStore for SqliteBackend {
    async fn save(&self, task: &Task) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO tasks (id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            rusqlite::params![
                task.id().to_string(),
                task.namespace().to_string(),
                task.title(),
                task.description(),
                task.status().to_string(),
                task.priority().to_string(),
                serde_json::to_string(task.assigned_roles()).unwrap(),
                task.claimed_by().map(|a| a.to_string()),
                task.claimed_at().map(|dt| dt.to_rfc3339()),
                serde_json::to_string(&task.depends_on().iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap(),
                task.result_summary().map(|s| s.to_string()),
                serde_json::to_string(&task.notes()).unwrap(),
                task.created_by().map(|a| a.to_string()),
                task.created_at().to_rfc3339(),
                task.updated_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], row_to_task)
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut sql = String::from(
            "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at
             FROM tasks WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{} || '/%')",
                idx
            ));
            params.push(Box::new(ns.to_string()));
            idx += 1;
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(
                " AND (namespace = ?{idx} OR namespace LIKE ?{idx} || '/%')"
            ));
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
                " AND (assigned_roles = '[]' OR assigned_roles LIKE '%' || ?{idx} || '%')"
            ));
            params.push(Box::new(role.clone()));
            idx += 1;
        }
        if let Some(ref claimed) = filter.claimed_by {
            sql.push_str(&format!(" AND claimed_by = ?{idx}"));
            params.push(Box::new(claimed.to_string()));
        }

        sql.push_str(
            " ORDER BY CASE priority
                WHEN 'critical' THEN 3
                WHEN 'high' THEN 2
                WHEN 'normal' THEN 1
                WHEN 'low' THEN 0
                ELSE 1
              END DESC",
        );

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let tasks = stmt
            .query_map(param_refs.as_slice(), row_to_task)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(tasks)
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    let id_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let title: String = row.get(2)?;
    let description: String = row.get(3)?;
    let status_str: String = row.get(4)?;
    let priority_str: String = row.get(5)?;
    let roles_str: String = row.get(6)?;
    let claimed_by_str: Option<String> = row.get(7)?;
    let claimed_at_str: Option<String> = row.get(8)?;
    let depends_on_str: String = row.get(9)?;
    let result_summary: Option<String> = row.get(10)?;
    let notes_str: String = row.get(11)?;
    let created_by_str: Option<String> = row.get(12)?;
    let created_at_str: String = row.get(13)?;
    let updated_at_str: String = row.get(14)?;

    let depends_on_strs: Vec<String> = serde_json::from_str(&depends_on_str).unwrap_or_default();
    let depends_on: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| TaskId::from_str(s).ok())
        .collect();

    let notes: Vec<Note> = serde_json::from_str(&notes_str).unwrap_or_default();

    let id = TaskId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let status = parse_task_status(&status_str);
    let priority = priority_str.parse::<Priority>().unwrap_or_default();
    let assigned_roles: Vec<String> = serde_json::from_str(&roles_str).unwrap_or_default();
    let claimed_by = claimed_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let claimed_at = claimed_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let created_by = created_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(13, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Task::restore(
        id,
        namespace,
        title,
        description,
        status,
        priority,
        assigned_roles,
        claimed_by,
        claimed_at,
        depends_on,
        result_summary,
        notes,
        created_by,
        created_at,
        updated_at,
    ))
}

fn parse_task_status(s: &str) -> TaskStatus {
    match s {
        "pending" => TaskStatus::Pending,
        "blocked" => TaskStatus::Blocked,
        "claimed" => TaskStatus::Claimed,
        "in_progress" => TaskStatus::InProgress,
        "completed" => TaskStatus::Completed,
        "failed" => TaskStatus::Failed,
        _ => TaskStatus::Pending,
    }
}
