use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::entities::{CreateTask, Task, TaskFilter};
use orchy_core::error::{Error, Result};
use orchy_core::store::TaskStore;
use orchy_core::value_objects::{AgentId, Namespace, Priority, TaskId, TaskStatus};

use crate::PgBackend;

impl TaskStore for PgBackend {
    async fn create(&self, cmd: CreateTask) -> Result<Task> {
        let now = Utc::now();
        let id = TaskId::new();

        // Determine initial status based on dependencies
        let initial_status = if cmd.depends_on.is_empty() {
            TaskStatus::Pending
        } else {
            let dep_ids: Vec<Uuid> = cmd.depends_on.iter().map(|d| *d.as_uuid()).collect();
            let row = sqlx::query(
                "SELECT COUNT(*) as cnt FROM tasks WHERE id = ANY($1) AND status = 'completed'",
            )
            .bind(&dep_ids)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

            let completed_count: i64 = row.get("cnt");
            if completed_count as usize == cmd.depends_on.len() {
                TaskStatus::Pending
            } else {
                TaskStatus::Blocked
            }
        };

        let roles_json = serde_json::to_value(&cmd.assigned_roles).unwrap();
        let depends_json = serde_json::to_value(
            &cmd.depends_on.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
        )
        .unwrap();

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

        sqlx::query(
            "INSERT INTO tasks (id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
        )
        .bind(task.id.as_uuid())
        .bind(task.namespace.to_string())
        .bind(&task.title)
        .bind(&task.description)
        .bind(task.status.to_string())
        .bind(task.priority.to_string())
        .bind(&roles_json)
        .bind(task.claimed_by.map(|a| *a.as_uuid()))
        .bind(task.claimed_at)
        .bind(&depends_json)
        .bind(&task.result_summary)
        .bind(task.created_by.map(|a| *a.as_uuid()))
        .bind(task.created_at)
        .bind(task.updated_at)
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(task)
    }

    async fn get(&self, id: &TaskId) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
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
            "SELECT id, namespace, title, description, status, priority, assigned_roles, claimed_by, claimed_at, depends_on, result_summary, created_by, created_at, updated_at
             FROM tasks WHERE 1=1",
        );
        let mut param_idx = 1u32;

        // We'll build params dynamically. Since sqlx doesn't support dynamic binding easily
        // with mixed types, we'll use a QueryBuilder approach or string params.
        // Actually let's use separate vecs and a manual approach with format strings,
        // then bind at the end using query_scalar style. But sqlx raw queries need concrete binds.
        // Simplest: build SQL with placeholders and use a Vec of string params.

        let mut ns_val: Option<String> = None;
        let mut status_val: Option<String> = None;
        let mut role_val: Option<String> = None;
        let mut claimed_val: Option<Uuid> = None;

        if let Some(ref ns) = filter.namespace {
            sql.push_str(&format!(
                " AND (namespace = ${param_idx} OR namespace LIKE ${param_idx} || '/%')"
            ));
            ns_val = Some(ns.to_string());
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
            // param_idx += 1;
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

    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let row = sqlx::query("SELECT status FROM tasks WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let status_str: String = row.get("status");
        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Claimed)?;

        let now = Utc::now();
        sqlx::query(
            "UPDATE tasks SET status = 'claimed', claimed_by = $1, claimed_at = $2, updated_at = $3 WHERE id = $4",
        )
        .bind(agent.as_uuid())
        .bind(now)
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let row = sqlx::query("SELECT status FROM tasks WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let status_str: String = row.get("status");
        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Completed)?;

        let now = Utc::now();
        sqlx::query(
            "UPDATE tasks SET status = 'completed', result_summary = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(&summary)
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let row = sqlx::query("SELECT status FROM tasks WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let status_str: String = row.get("status");
        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Failed)?;

        let now = Utc::now();
        sqlx::query(
            "UPDATE tasks SET status = 'failed', result_summary = $1, updated_at = $2 WHERE id = $3",
        )
        .bind(&reason)
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    async fn release(&self, id: &TaskId) -> Result<Task> {
        let row = sqlx::query("SELECT status FROM tasks WHERE id = $1")
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let status_str: String = row.get("status");
        let current = parse_task_status(&status_str);
        current.transition_to(TaskStatus::Pending)?;

        let now = Utc::now();
        sqlx::query(
            "UPDATE tasks SET status = 'pending', claimed_by = NULL, claimed_at = NULL, updated_at = $1 WHERE id = $2",
        )
        .bind(now)
        .bind(id.as_uuid())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        self.get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        let result = sqlx::query("UPDATE tasks SET status = $1, updated_at = $2 WHERE id = $3")
            .bind(status.to_string())
            .bind(Utc::now())
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(Error::NotFound(format!("task {id}")));
        }
        Ok(())
    }
}

fn row_to_task(row: &sqlx::postgres::PgRow) -> Task {
    let id: Uuid = row.get("id");
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
    let created_by: Option<Uuid> = row.get("created_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");

    let depends_on_strs: Vec<String> = serde_json::from_value(depends_on).unwrap_or_default();
    let depends_on_ids: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    Task {
        id: TaskId::from_uuid(id),
        namespace: Namespace::try_from(namespace).unwrap(),
        title,
        description,
        status: parse_task_status(&status),
        priority: priority.parse::<Priority>().unwrap_or_default(),
        assigned_roles: serde_json::from_value(assigned_roles).unwrap_or_default(),
        claimed_by: claimed_by.map(AgentId::from_uuid),
        claimed_at,
        depends_on: depends_on_ids,
        result_summary,
        created_by: created_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    }
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
