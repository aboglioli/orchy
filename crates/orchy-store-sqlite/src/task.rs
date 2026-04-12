use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::entities::{CreateTask, Task, TaskFilter};
use orchy_core::error::{Error, Result};
use orchy_core::store::TaskStore;
use orchy_core::value_objects::{AgentId, Namespace, Priority, TaskId, TaskStatus};

use crate::SqliteBackend;

impl TaskStore for SqliteBackend {
    async fn create(&self, cmd: CreateTask) -> Result<Task> {
        let now = Utc::now();
        let id = TaskId::new();

        // Determine initial status based on dependencies
        let initial_status = if cmd.depends_on.is_empty() {
            TaskStatus::Pending
        } else {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
            let all_completed = cmd.depends_on.iter().all(|dep_id| {
                let status: Option<String> = conn
                    .query_row(
                        "SELECT status FROM tasks WHERE id = ?1",
                        rusqlite::params![dep_id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()
                    .unwrap_or(None);
                status.as_deref() == Some("completed")
            });
            if all_completed {
                TaskStatus::Pending
            } else {
                TaskStatus::Blocked
            }
        };

        let task = Task {
            id,
            namespace: cmd.namespace,
            title: cmd.title,
            description: cmd.description,
            status: initial_status,
            priority: cmd.priority,
            assigned_roles: cmd.assigned_roles,
            claimed_by: None,
            claimed_at: None,
            depends_on: cmd.depends_on,
            result_summary: None,
            created_by: cmd.created_by,
            created_at: now,
            updated_at: now,
        };

        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT INTO tasks (id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![
                task.id.to_string(),
                task.namespace.to_string(),
                task.title,
                task.description,
                task.status.to_string(),
                task.priority.to_string(),
                serde_json::to_string(&task.assigned_roles).unwrap(),
                task.claimed_by.map(|a| a.to_string()),
                task.claimed_at.map(|dt| dt.to_rfc3339()),
                serde_json::to_string(&task.depends_on.iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap(),
                task.result_summary,
                task.created_by.map(|a| a.to_string()),
                task.created_at.to_rfc3339(),
                task.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(task)
    }

    async fn get(&self, id: &TaskId) -> Result<Option<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
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
            "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
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
        if let Some(ref status) = filter.status {
            sql.push_str(&format!(" AND status = ?{idx}"));
            params.push(Box::new(status.to_string()));
            idx += 1;
        }
        if let Some(ref role) = filter.assigned_role {
            // Match tasks where assigned_roles is empty (any role) OR contains the role
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

        // Sort by priority descending: Critical(3) > High(2) > Normal(1) > Low(0)
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

    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare("SELECT status FROM tasks WHERE id = ?1")
            .map_err(|e| Error::Store(e.to_string()))?;
        let status_str: String = stmt
            .query_row(rusqlite::params![id.to_string()], |row| row.get(0))
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Claimed)?;

        let now = Utc::now();
        conn.execute(
            "UPDATE tasks SET status = 'claimed', claimed_by = ?1, claimed_at = ?2, updated_at = ?3 WHERE id = ?4",
            rusqlite::params![agent.to_string(), now.to_rfc3339(), now.to_rfc3339(), id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        drop(stmt);
        let mut stmt2 = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt2
            .query_row(rusqlite::params![id.to_string()], row_to_task)
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let status_str: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Completed)?;

        let now = Utc::now();
        conn.execute(
            "UPDATE tasks SET status = 'completed', result_summary = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![summary, now.to_rfc3339(), id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(rusqlite::params![id.to_string()], row_to_task)
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let status_str: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Failed)?;

        let now = Utc::now();
        conn.execute(
            "UPDATE tasks SET status = 'failed', result_summary = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![reason, now.to_rfc3339(), id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(rusqlite::params![id.to_string()], row_to_task)
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn release(&self, id: &TaskId) -> Result<Task> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let status_str: String = conn
            .query_row(
                "SELECT status FROM tasks WHERE id = ?1",
                rusqlite::params![id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Pending)?;

        let now = Utc::now();
        conn.execute(
            "UPDATE tasks SET status = 'pending', claimed_by = NULL, claimed_at = NULL, updated_at = ?1 WHERE id = ?2",
            rusqlite::params![now.to_rfc3339(), id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
                 FROM tasks WHERE id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_row(rusqlite::params![id.to_string()], row_to_task)
            .map_err(|e| Error::Store(e.to_string()))
    }

    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let rows = conn
            .execute(
                "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![status.to_string(), Utc::now().to_rfc3339(), id.to_string()],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        if rows == 0 {
            return Err(Error::NotFound(format!("task {id}")));
        }
        Ok(())
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
    let created_by_str: Option<String> = row.get(11)?;
    let created_at_str: String = row.get(12)?;
    let updated_at_str: String = row.get(13)?;

    let depends_on_strs: Vec<String> = serde_json::from_str(&depends_on_str).unwrap_or_default();
    let depends_on: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| TaskId::from_str(s).ok())
        .collect();

    Ok(Task {
        id: TaskId::from_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?,
        namespace: Namespace::try_from(namespace_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
            )
        })?,
        title,
        description,
        status: parse_task_status(&status_str),
        priority: priority_str.parse::<Priority>().unwrap_or_default(),
        assigned_roles: serde_json::from_str(&roles_str).unwrap_or_default(),
        claimed_by: claimed_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        claimed_at: claimed_at_str
            .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
            .map(|dt| dt.with_timezone(&Utc)),
        depends_on,
        result_summary,
        created_by: created_by_str.and_then(|s| AgentId::from_str(&s).ok()),
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    12,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
        updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    13,
                    rusqlite::types::Type::Text,
                    Box::new(e),
                )
            })?,
    })
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
