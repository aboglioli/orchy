use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_query::{Value, Values};
use sea_query_binder::SqlxValues;
use sqlx::{PgPool, Row, postgres::PgRow};
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result, StoreError};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};
use orchy_events::io::Writer;

use crate::{decode_json_value, events::PgEventWriter, parse_namespace, parse_project_id};

pub struct PgTaskStore {
    pool: PgPool,
}

impl PgTaskStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TaskStore for PgTaskStore {
    async fn save(&self, task: &mut Task) -> Result<()> {
        let roles_json = serde_json::to_value(task.assigned_roles()).map_err(|e| {
            Error::Store(StoreError::Serialization(format!(
                "tasks.assigned_roles: {e}"
            )))
        })?;
        let tags_json = serde_json::to_value(task.tags())
            .map_err(|e| Error::Store(StoreError::Serialization(format!("tasks.tags: {e}"))))?;
        let mut tx = self.pool.begin().await.map_err(crate::error::store_err)?;

        if let Some(pv) = task.persisted_version() {
            let result = sqlx::query(
                "UPDATE tasks SET organization_id=$2, project=$3, namespace=$4, title=$5, \
                 description=$6, acceptance_criteria=$7, status=$8, priority=$9, \
                 assigned_roles=$10, assigned_to=$11, assigned_at=$12, stale_after_secs=$13, \
                 last_activity_at=$14, tags=$15, result_summary=$16, archived_at=$17, \
                 created_by=$18, created_at=$19, updated_at=$20, version=$21 \
                 WHERE id=$1 AND version=$22",
            )
            .bind(task.id().as_uuid())
            .bind(task.org_id().to_string())
            .bind(task.project().to_string())
            .bind(task.namespace().to_string())
            .bind(task.title())
            .bind(task.description())
            .bind(task.acceptance_criteria())
            .bind(task.status().to_string())
            .bind(task.priority().to_string())
            .bind(&roles_json)
            .bind(task.assigned_to().map(|a| *a.as_uuid()))
            .bind(task.assigned_at())
            .bind(task.stale_after_secs().map(|v| v as i64))
            .bind(task.last_activity_at())
            .bind(&tags_json)
            .bind(task.result_summary())
            .bind(task.archived_at())
            .bind(task.created_by().map(|a| *a.as_uuid()))
            .bind(task.created_at())
            .bind(task.updated_at())
            .bind(task.version() as i64)
            .bind(pv as i64)
            .execute(&mut *tx)
            .await
            .map_err(crate::error::store_err)?;

            if result.rows_affected() == 0 {
                return Err(Error::version_mismatch(pv, task.version()));
            }
        } else {
            sqlx::query(
                "INSERT INTO tasks (id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at, version)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21)
                 ON CONFLICT (id) DO UPDATE SET
                    organization_id = EXCLUDED.organization_id,
                    project = EXCLUDED.project,
                    namespace = EXCLUDED.namespace,
                    title = EXCLUDED.title,
                    description = EXCLUDED.description,
                    acceptance_criteria = EXCLUDED.acceptance_criteria,
                    status = EXCLUDED.status,
                    priority = EXCLUDED.priority,
                    assigned_roles = EXCLUDED.assigned_roles,
                    assigned_to = EXCLUDED.assigned_to,
                    assigned_at = EXCLUDED.assigned_at,
                    stale_after_secs = EXCLUDED.stale_after_secs,
                    last_activity_at = EXCLUDED.last_activity_at,
                    tags = EXCLUDED.tags,
                    result_summary = EXCLUDED.result_summary,
                    archived_at = EXCLUDED.archived_at,
                    updated_at = EXCLUDED.updated_at,
                    version = EXCLUDED.version",
            )
            .bind(task.id().as_uuid())
            .bind(task.org_id().to_string())
            .bind(task.project().to_string())
            .bind(task.namespace().to_string())
            .bind(task.title())
            .bind(task.description())
            .bind(task.acceptance_criteria())
            .bind(task.status().to_string())
            .bind(task.priority().to_string())
            .bind(&roles_json)
            .bind(task.assigned_to().map(|a| *a.as_uuid()))
            .bind(task.assigned_at())
            .bind(task.stale_after_secs().map(|v| v as i64))
            .bind(task.last_activity_at())
            .bind(&tags_json)
            .bind(task.result_summary())
            .bind(task.archived_at())
            .bind(task.created_by().map(|a| *a.as_uuid()))
            .bind(task.created_at())
            .bind(task.updated_at())
            .bind(task.version() as i64)
            .execute(&mut *tx)
            .await
            .map_err(crate::error::store_err)?;
        }

        task.mark_persisted();

        let events = task.drain_events();
        PgEventWriter::new_tx(&mut tx).write_all(&events).await?;

        tx.commit().await.map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at, version
             FROM tasks WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(crate::error::store_err)?;

        row.map(|r| row_to_task(&r)).transpose()
    }

    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let uuid_ids: Vec<Uuid> = ids.iter().map(|id| *id.as_uuid()).collect();
        let rows = sqlx::query(
            "SELECT id, organization_id, project, namespace, title, description, \
             acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, \
             stale_after_secs, last_activity_at, tags, result_summary, archived_at, \
             created_by, created_at, updated_at, version \
             FROM tasks WHERE id = ANY($1::uuid[])",
        )
        .bind(&uuid_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(crate::error::store_err)?;
        rows.iter().map(row_to_task).collect()
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        let select_cols = "id, organization_id, project, namespace, title, description, \
            acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, \
            stale_after_secs, last_activity_at, tags, result_summary, archived_at, \
            created_by, created_at, updated_at, version";

        let mut conditions: Vec<String> = Vec::new();
        let mut values: Vec<Value> = Vec::new();
        let mut param_idx = 1usize;

        if let Some(ref org_id) = filter.org_id {
            conditions.push(format!("organization_id = ${}", param_idx));
            values.push(Value::String(Some(Box::new(org_id.to_string()))));
            param_idx += 1;
        }
        if let Some(ref ns) = filter.namespace
            && !ns.is_root()
        {
            conditions.push(format!(
                "(namespace = ${0} OR namespace LIKE ${0} || '/%')",
                param_idx
            ));
            values.push(Value::String(Some(Box::new(ns.to_string()))));
            param_idx += 1;
        }
        if let Some(ref project) = filter.project {
            conditions.push(format!("project = ${}", param_idx));
            values.push(Value::String(Some(Box::new(project.to_string()))));
            param_idx += 1;
        }
        if let Some(ref status) = filter.status {
            conditions.push(format!("status = ${}", param_idx));
            values.push(Value::String(Some(Box::new(status.to_string()))));
            param_idx += 1;
        }
        if let Some(ref role) = filter.assigned_role {
            conditions.push(format!(
                "(assigned_roles = '[]'::jsonb OR assigned_roles @> jsonb_build_array(${}::text))",
                param_idx
            ));
            values.push(Value::String(Some(Box::new(role.clone()))));
            param_idx += 1;
        }
        if let Some(ref assigned) = filter.assigned_to {
            conditions.push(format!("assigned_to = ${}", param_idx));
            values.push(Value::Uuid(Some(Box::new(*assigned.as_uuid()))));
            param_idx += 1;
        }
        if let Some(ref tag) = filter.tag {
            conditions.push(format!("tags @> jsonb_build_array(${}::text)", param_idx));
            values.push(Value::String(Some(Box::new(tag.clone()))));
            param_idx += 1;
        }
        if !filter.include_archived.unwrap_or(false) {
            conditions.push("archived_at IS NULL".to_string());
        }

        if let Some(ref cursor) = page.after
            && let Some(decoded) = decode_cursor(cursor)
            && let Ok(cursor_uuid) = decoded.parse::<Uuid>()
        {
            conditions.push(format!("id < ${}", param_idx));
            values.push(Value::Uuid(Some(Box::new(cursor_uuid))));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT {} FROM tasks {} \
             ORDER BY \
               CASE priority \
                 WHEN 'critical' THEN 3 WHEN 'high' THEN 2 \
                 WHEN 'normal' THEN 1 WHEN 'low' THEN 0 \
                 ELSE 1 \
               END DESC, \
             id DESC \
             LIMIT {}",
            select_cols,
            where_clause,
            (page.limit as u64).saturating_add(1)
        );

        let rows = sqlx::query_with(&sql, SqlxValues(Values(values)))
            .fetch_all(&self.pool)
            .await
            .map_err(crate::error::store_err)?;

        let mut tasks: Vec<Task> = rows.iter().map(row_to_task).collect::<Result<Vec<_>>>()?;

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

fn row_to_task(row: &PgRow) -> Result<Task> {
    let id: Uuid = row.get("id");
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let title: String = row.get("title");
    let description: String = row.get("description");
    let acceptance_criteria: Option<String> = row.get("acceptance_criteria");
    let status: String = row.get("status");
    let priority: String = row.get("priority");
    let assigned_roles: serde_json::Value = row.get("assigned_roles");
    let assigned_to: Option<Uuid> = row.get("assigned_to");
    let assigned_at: Option<DateTime<Utc>> = row.get("assigned_at");
    let stale_after_secs: Option<i64> = row.get("stale_after_secs");
    let last_activity_at: DateTime<Utc> = row.get("last_activity_at");
    let tags: serde_json::Value = row.get("tags");
    let result_summary: Option<String> = row.get("result_summary");
    let archived_at: Option<DateTime<Utc>> = row.get("archived_at");
    let created_by: Option<Uuid> = row.get("created_by");
    let created_at: DateTime<Utc> = row.get("created_at");
    let updated_at: DateTime<Utc> = row.get("updated_at");
    let version: i64 = row.get("version");

    Ok(Task::restore(RestoreTask {
        id: TaskId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str).map_err(|e| {
            Error::Store(StoreError::Serialization(format!(
                "tasks.organization_id: {e}"
            )))
        })?,
        project: parse_project_id(project, "tasks", "project")?,
        namespace: parse_namespace(namespace, "tasks", "namespace")?,
        title,
        description,
        acceptance_criteria,
        status: status.parse::<TaskStatus>().unwrap_or(TaskStatus::Pending),
        priority: priority.parse::<Priority>().unwrap_or_default(),
        assigned_roles: decode_json_value(assigned_roles, "tasks", "assigned_roles")?,
        assigned_to: assigned_to.map(AgentId::from_uuid),
        assigned_at,
        stale_after_secs: stale_after_secs.map(|v| v as u64),
        last_activity_at,
        tags: decode_json_value(tags, "tasks", "tags")?,
        result_summary,
        archived_at,
        created_by: created_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
        version: version as u64,
    }))
}
