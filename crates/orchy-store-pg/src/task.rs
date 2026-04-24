use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::{PgPool, Row};
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams, decode_cursor, encode_cursor};
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};
use orchy_events::io::Writer;

use crate::{decode_json_value, events::PgEventWriter, parse_namespace, parse_project_id};

#[derive(Iden)]
enum Tasks {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "organization_id"]
    OrganizationId,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "title"]
    Title,
    #[iden = "description"]
    Description,
    #[iden = "acceptance_criteria"]
    AcceptanceCriteria,
    #[iden = "status"]
    Status,
    #[iden = "priority"]
    Priority,
    #[iden = "assigned_roles"]
    AssignedRoles,
    #[iden = "assigned_to"]
    AssignedTo,
    #[iden = "assigned_at"]
    AssignedAt,
    #[iden = "stale_after_secs"]
    StaleAfterSecs,
    #[iden = "last_activity_at"]
    LastActivityAt,
    #[iden = "tags"]
    Tags,
    #[iden = "result_summary"]
    ResultSummary,
    #[iden = "archived_at"]
    ArchivedAt,
    #[iden = "created_by"]
    CreatedBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

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
        let roles_json = serde_json::to_value(task.assigned_roles())
            .map_err(|e| Error::Store(format!("failed to serialize tasks.assigned_roles: {e}")))?;
        let tags_json = serde_json::to_value(task.tags())
            .map_err(|e| Error::Store(format!("failed to serialize tasks.tags: {e}")))?;
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO tasks (id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20)
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
                updated_at = EXCLUDED.updated_at",
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
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = task.drain_events();
        PgEventWriter::new_tx(&mut tx)
            .write_all(&events)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let row = sqlx::query(
            "SELECT id, organization_id, project, namespace, title, description, acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at
             FROM tasks WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        row.map(|r| row_to_task(&r)).transpose()
    }

    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let uuid_ids: Vec<uuid::Uuid> = ids.iter().map(|id| *id.as_uuid()).collect();
        let rows = sqlx::query(
            "SELECT id, organization_id, project, namespace, title, description, \
             acceptance_criteria, status, priority, assigned_roles, assigned_to, assigned_at, \
             stale_after_secs, last_activity_at, tags, result_summary, archived_at, created_by, created_at, updated_at \
             FROM tasks WHERE id = ANY($1::uuid[])",
        )
        .bind(&uuid_ids)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        rows.iter().map(row_to_task).collect()
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        let mut select = Query::select();
        select.from(Tasks::Table).columns([
            Tasks::Id,
            Tasks::OrganizationId,
            Tasks::Project,
            Tasks::Namespace,
            Tasks::Title,
            Tasks::Description,
            Tasks::AcceptanceCriteria,
            Tasks::Status,
            Tasks::Priority,
            Tasks::AssignedRoles,
            Tasks::AssignedTo,
            Tasks::AssignedAt,
            Tasks::StaleAfterSecs,
            Tasks::LastActivityAt,
            Tasks::Tags,
            Tasks::ResultSummary,
            Tasks::ArchivedAt,
            Tasks::CreatedBy,
            Tasks::CreatedAt,
            Tasks::UpdatedAt,
        ]);

        if let Some(ref org_id) = filter.org_id {
            select.and_where(Expr::col(Tasks::OrganizationId).eq(org_id.to_string()));
        }
        if let Some(ref ns) = filter.namespace
            && !ns.is_root()
        {
            select.cond_where(
                Cond::any()
                    .add(Expr::col(Tasks::Namespace).eq(ns.to_string()))
                    .add(Expr::col(Tasks::Namespace).like(format!("{}/%", ns))),
            );
        }
        if let Some(ref project) = filter.project {
            select.and_where(Expr::col(Tasks::Project).eq(project.to_string()));
        }
        if let Some(ref status) = filter.status {
            select.and_where(Expr::col(Tasks::Status).eq(status.to_string()));
        }
        if let Some(ref role) = filter.assigned_role {
            select.and_where(Expr::cust_with_values(
                "(assigned_roles = '[]'::jsonb OR assigned_roles @> to_jsonb(?::text))",
                [sea_query::Value::String(Some(Box::new(role.clone())))],
            ));
        }
        if let Some(ref assigned) = filter.assigned_to {
            select.and_where(Expr::col(Tasks::AssignedTo).eq(*assigned.as_uuid()));
        }
        if let Some(ref tag) = filter.tag {
            select.and_where(Expr::cust_with_values(
                "tags @> to_jsonb(?::text)",
                [sea_query::Value::String(Some(Box::new(tag.clone())))],
            ));
        }
        if !filter.include_archived.unwrap_or(false) {
            select.and_where(Expr::col(Tasks::ArchivedAt).is_null());
        }

        if let Some(ref cursor) = page.after
            && let Some(decoded) = decode_cursor(cursor)
            && let Ok(cursor_uuid) = decoded.parse::<Uuid>()
        {
            select.and_where(Expr::col(Tasks::Id).lt(cursor_uuid));
        }

        select
            .order_by_expr(
                Expr::cust(
                    "CASE priority WHEN 'critical' THEN 3 WHEN 'high' THEN 2 WHEN 'normal' THEN 1 WHEN 'low' THEN 0 ELSE 1 END",
                ),
                sea_query::Order::Desc,
            )
            .order_by(Tasks::Id, sea_query::Order::Desc)
            .limit((page.limit as u64).saturating_add(1));

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

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

fn row_to_task(row: &sqlx::postgres::PgRow) -> Result<Task> {
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

    Ok(Task::restore(RestoreTask {
        id: TaskId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid tasks.organization_id: {e}")))?,
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
    }))
}
