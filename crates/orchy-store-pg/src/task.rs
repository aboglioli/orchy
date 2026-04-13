use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::note::Note;
use orchy_core::task::{Priority, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::PgBackend;

impl TaskStore for PgBackend {
    async fn save(&self, task: &Task) -> Result<()> {
        let roles_json = serde_json::to_value(task.assigned_roles()).unwrap();
        let depends_json = serde_json::to_value(
            task.depends_on()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .unwrap();
        let notes_json = serde_json::to_value(task.notes()).unwrap();

        sqlx::query(
            "INSERT INTO tasks (id, project, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
             ON CONFLICT (id) DO UPDATE SET
                namespace = EXCLUDED.namespace,
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                priority = EXCLUDED.priority,
                assigned_roles = EXCLUDED.assigned_roles,
                claimed_by = EXCLUDED.claimed_by,
                claimed_at = EXCLUDED.claimed_at,
                depends_on = EXCLUDED.depends_on,
                result_summary = EXCLUDED.result_summary,
                notes = EXCLUDED.notes,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(task.id().as_uuid())
        .bind(task.project().to_string())
        .bind(task.namespace().to_string())
        .bind(task.title())
        .bind(task.description())
        .bind(task.status().to_string())
        .bind(task.priority().to_string())
        .bind(&roles_json)
        .bind(task.claimed_by().map(|a| *a.as_uuid()))
        .bind(task.claimed_at())
        .bind(&depends_json)
        .bind(task.result_summary())
        .bind(&notes_json)
        .bind(task.created_by().map(|a| *a.as_uuid()))
        .bind(task.created_at())
        .bind(task.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, project, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at
             FROM tasks WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_task(&r)))
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let mut sql = String::from(
            "SELECT id, project, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, notes, created_by, created_at, updated_at
             FROM tasks WHERE 1=1",
        );
        let mut param_idx = 1u32;

        let mut ns_val: Option<String> = None;
        let mut project_val: Option<String> = None;
        let mut status_val: Option<String> = None;
        let mut role_val: Option<String> = None;
        let mut claimed_val: Option<Uuid> = None;

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                sql.push_str(&format!(
                    " AND (namespace = ${param_idx} OR namespace LIKE ${param_idx} || '/%')"
                ));
                ns_val = Some(ns.to_string());
                param_idx += 1;
            }
        }
        if let Some(ref project) = filter.project {
            sql.push_str(&format!(" AND project = ${param_idx}"));
            project_val = Some(project.to_string());
            param_idx += 1;
        }
        if let Some(ref status) = filter.status {
            sql.push_str(&format!(" AND status = ${param_idx}"));
            status_val = Some(status.to_string());
            param_idx += 1;
        }
        if let Some(ref role) = filter.assigned_role {
            sql.push_str(&format!(
                " AND (assigned_roles = '[]'::jsonb OR assigned_roles @> to_jsonb(${param_idx}::text))"
            ));
            role_val = Some(role.clone());
            param_idx += 1;
        }
        if let Some(ref claimed) = filter.claimed_by {
            sql.push_str(&format!(" AND claimed_by = ${param_idx}"));
            claimed_val = Some(*claimed.as_uuid());
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

        let mut query = sqlx::query(&sql);
        if let Some(ref v) = ns_val {
            query = query.bind(v);
        }
        if let Some(ref v) = project_val {
            query = query.bind(v);
        }
        if let Some(ref v) = status_val {
            query = query.bind(v);
        }
        if let Some(ref v) = role_val {
            query = query.bind(v);
        }
        if let Some(v) = claimed_val {
            query = query.bind(v);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_task).collect())
    }
}

fn row_to_task(row: &sqlx::postgres::PgRow) -> Task {
    let id: Uuid = row.get("id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let title: String = row.get("title");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let priority: String = row.get("priority");
    let assigned_roles: serde_json::Value = row.get("assigned_roles");
    let claimed_by: Option<Uuid> = row.get("claimed_by");
    let claimed_at: Option<DateTime<Utc>> = row.get("claimed_at");
    let depends_on: serde_json::Value = row.get("depends_on");
    let result_summary: Option<String> = row.get("result_summary");
    let notes_json: serde_json::Value = row.get("notes");
    let created_by: Option<Uuid> = row.get("created_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let depends_on_strs: Vec<String> = serde_json::from_value(depends_on).unwrap_or_default();
    let depends_on_ids: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();
    let notes: Vec<Note> = serde_json::from_value(notes_json).unwrap_or_default();

    Task::restore(
        TaskId::from_uuid(id),
        ProjectId::try_from(project).expect("invalid project in database"),
        Namespace::try_from(namespace).unwrap(),
        title,
        description,
        parse_task_status(&status),
        priority.parse::<Priority>().unwrap_or_default(),
        serde_json::from_value(assigned_roles).unwrap_or_default(),
        claimed_by.map(AgentId::from_uuid),
        claimed_at,
        depends_on_ids,
        result_summary,
        notes,
        created_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    )
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
