use std::str::FromStr;

use chrono::{DateTime, Utc};
use sea_query::{Cond, Expr, Iden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::note::Note;
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::PgBackend;

#[derive(Iden)]
enum Tasks {
    Table,
    #[iden = "id"]
    Id,
    #[iden = "project"]
    Project,
    #[iden = "namespace"]
    Namespace,
    #[iden = "parent_id"]
    ParentId,
    #[iden = "title"]
    Title,
    #[iden = "description"]
    Description,
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
    #[iden = "depends_on"]
    DependsOn,
    #[iden = "result_summary"]
    ResultSummary,
    #[iden = "notes"]
    Notes,
    #[iden = "created_by"]
    CreatedBy,
    #[iden = "created_at"]
    CreatedAt,
    #[iden = "updated_at"]
    UpdatedAt,
}

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
            "INSERT INTO tasks (id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, result_summary, notes, created_by, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
             ON CONFLICT (id) DO UPDATE SET
                namespace = EXCLUDED.namespace,
                parent_id = EXCLUDED.parent_id,
                title = EXCLUDED.title,
                description = EXCLUDED.description,
                status = EXCLUDED.status,
                priority = EXCLUDED.priority,
                assigned_roles = EXCLUDED.assigned_roles,
                assigned_to = EXCLUDED.assigned_to,
                assigned_at = EXCLUDED.assigned_at,
                depends_on = EXCLUDED.depends_on,
                result_summary = EXCLUDED.result_summary,
                notes = EXCLUDED.notes,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(task.id().as_uuid())
        .bind(task.project().to_string())
        .bind(task.namespace().to_string())
        .bind(task.parent_id().map(|id| *id.as_uuid()))
        .bind(task.title())
        .bind(task.description())
        .bind(task.status().to_string())
        .bind(task.priority().to_string())
        .bind(&roles_json)
        .bind(task.assigned_to().map(|a| *a.as_uuid()))
        .bind(task.assigned_at())
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
            "SELECT id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, result_summary, notes, created_by, created_at, updated_at
             FROM tasks WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(row.map(|r| row_to_task(&r)))
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let mut select = Query::select();
        select
            .from(Tasks::Table)
            .columns([
                Tasks::Id,
                Tasks::Project,
                Tasks::Namespace,
                Tasks::ParentId,
                Tasks::Title,
                Tasks::Description,
                Tasks::Status,
                Tasks::Priority,
                Tasks::AssignedRoles,
                Tasks::AssignedTo,
                Tasks::AssignedAt,
                Tasks::DependsOn,
                Tasks::ResultSummary,
                Tasks::Notes,
                Tasks::CreatedBy,
                Tasks::CreatedAt,
                Tasks::UpdatedAt,
            ]);

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                select.cond_where(
                    Cond::any()
                        .add(Expr::col(Tasks::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Tasks::Namespace).like(format!("{}/%", ns))),
                );
            }
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
                [role.clone().into()],
            ));
        }
        if let Some(ref assigned) = filter.assigned_to {
            select.and_where(Expr::col(Tasks::AssignedTo).eq(*assigned.as_uuid()));
        }
        if let Some(ref pid) = filter.parent_id {
            select.and_where(Expr::col(Tasks::ParentId).eq(*pid.as_uuid()));
        }

        select.order_by_expr(
            Expr::cust(
                "CASE priority WHEN 'critical' THEN 3 WHEN 'high' THEN 2 WHEN 'normal' THEN 1 WHEN 'low' THEN 0 ELSE 1 END",
            ),
            sea_query::Order::Desc,
        );

        let (sql, values) = select.build_sqlx(PostgresQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
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
    let parent_id: Option<Uuid> = row.get("parent_id");
    let title: String = row.get("title");
    let description: String = row.get("description");
    let status: String = row.get("status");
    let priority: String = row.get("priority");
    let assigned_roles: serde_json::Value = row.get("assigned_roles");
    let assigned_to: Option<Uuid> = row.get("assigned_to");
    let assigned_at: Option<DateTime<Utc>> = row.get("assigned_at");
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

    Task::restore(RestoreTask {
        id: TaskId::from_uuid(id),
        project: ProjectId::try_from(project).expect("invalid project in database"),
        namespace: Namespace::try_from(namespace).unwrap(),
        parent_id: parent_id.map(TaskId::from_uuid),
        title,
        description,
        status: status.parse::<TaskStatus>().unwrap_or(TaskStatus::Pending),
        priority: priority.parse::<Priority>().unwrap_or_default(),
        assigned_roles: serde_json::from_value(assigned_roles).unwrap_or_default(),
        assigned_to: assigned_to.map(AgentId::from_uuid),
        assigned_at,
        depends_on: depends_on_ids,
        result_summary,
        notes,
        created_by: created_by.map(AgentId::from_uuid),
        created_at,
        updated_at,
    })
}