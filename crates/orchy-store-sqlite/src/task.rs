use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use sea_query::{Cond, Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_rusqlite::RusqliteBinder;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::note::Note;
use orchy_core::task::{Priority, RestoreTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::SqliteBackend;

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
    #[iden = "tags"]
    Tags,
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

impl TaskStore for SqliteBackend {
    async fn save(&self, task: &mut Task) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO tasks (id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, tags, result_summary, notes, created_by, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
                rusqlite::params![
                    task.id().to_string(),
                    task.project().to_string(),
                    task.namespace().to_string(),
                    task.parent_id().map(|id| id.to_string()),
                    task.title(),
                    task.description(),
                    task.status().to_string(),
                    task.priority().to_string(),
                    serde_json::to_string(task.assigned_roles()).unwrap(),
                    task.assigned_to().map(|a| a.to_string()),
                    task.assigned_at().map(|dt| dt.to_rfc3339()),
                    serde_json::to_string(&task.depends_on().iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap(),
                    serde_json::to_string(task.tags()).unwrap(),
                    task.result_summary().map(|s| s.to_string()),
                    serde_json::to_string(&task.notes()).unwrap(),
                    task.created_by().map(|a| a.to_string()),
                    task.created_at().to_rfc3339(),
                    task.updated_at().to_rfc3339(),
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = task.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, project, namespace, parent_id, title, description, status, priority, assigned_roles, assigned_to, assigned_at, depends_on, tags, result_summary, notes, created_by, created_at, updated_at
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

        let mut query = Query::select();
        query.from(Tasks::Table).columns([
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
            Tasks::Tags,
            Tasks::ResultSummary,
            Tasks::Notes,
            Tasks::CreatedBy,
            Tasks::CreatedAt,
            Tasks::UpdatedAt,
        ]);

        if let Some(ref ns) = filter.namespace {
            if !ns.is_root() {
                query.cond_where(
                    Cond::any()
                        .add(Expr::col(Tasks::Namespace).eq(ns.to_string()))
                        .add(Expr::col(Tasks::Namespace).like(format!("{}/%", ns))),
                );
            }
        }
        if let Some(ref project) = filter.project {
            query.and_where(Expr::col(Tasks::Project).eq(project.to_string()));
        }
        if let Some(ref status) = filter.status {
            query.and_where(Expr::col(Tasks::Status).eq(status.to_string()));
        }
        if let Some(ref role) = filter.assigned_role {
            query.cond_where(
                Cond::any()
                    .add(Expr::col(Tasks::AssignedRoles).eq("[]"))
                    .add(Expr::col(Tasks::AssignedRoles).like(format!("%{role}%"))),
            );
        }
        if let Some(ref assigned) = filter.assigned_to {
            query.and_where(Expr::col(Tasks::AssignedTo).eq(assigned.to_string()));
        }
        if let Some(ref pid) = filter.parent_id {
            query.and_where(Expr::col(Tasks::ParentId).eq(pid.to_string()));
        }
        if let Some(ref tag) = filter.tag {
            query.and_where(Expr::col(Tasks::Tags).like(format!("%{tag}%")));
        }

        query.order_by_expr(
            Expr::cust("CASE priority WHEN 'critical' THEN 3 WHEN 'high' THEN 2 WHEN 'normal' THEN 1 WHEN 'low' THEN 0 ELSE 1 END"),
            sea_query::Order::Desc,
        );

        let (sql, values) = query.build_rusqlite(SqliteQueryBuilder);
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| Error::Store(e.to_string()))?;
        let tasks = stmt
            .query_map(&*values.as_params(), row_to_task)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(tasks)
    }
}

fn row_to_task(row: &rusqlite::Row) -> rusqlite::Result<Task> {
    let id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let parent_id_str: Option<String> = row.get(3)?;
    let title: String = row.get(4)?;
    let description: String = row.get(5)?;
    let status_str: String = row.get(6)?;
    let priority_str: String = row.get(7)?;
    let roles_str: String = row.get(8)?;
    let assigned_to_str: Option<String> = row.get(9)?;
    let assigned_at_str: Option<String> = row.get(10)?;
    let depends_on_str: String = row.get(11)?;
    let tags_str: String = row.get(12)?;
    let result_summary: Option<String> = row.get(13)?;
    let notes_str: String = row.get(14)?;
    let created_by_str: Option<String> = row.get(15)?;
    let created_at_str: String = row.get(16)?;
    let updated_at_str: String = row.get(17)?;

    let depends_on_strs: Vec<String> = serde_json::from_str(&depends_on_str).unwrap_or_default();
    let depends_on: Vec<TaskId> = depends_on_strs
        .iter()
        .filter_map(|s| TaskId::from_str(s).ok())
        .collect();

    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    let notes: Vec<Note> = serde_json::from_str(&notes_str).unwrap_or_default();

    let id = TaskId::from_str(&id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let status = status_str
        .parse::<TaskStatus>()
        .unwrap_or(TaskStatus::Pending);
    let priority = priority_str.parse::<Priority>().unwrap_or_default();
    let assigned_roles: Vec<String> = serde_json::from_str(&roles_str).unwrap_or_default();
    let parent_id = parent_id_str.and_then(|s| TaskId::from_str(&s).ok());
    let assigned_to = assigned_to_str.and_then(|s| AgentId::from_str(&s).ok());
    let assigned_at = assigned_at_str
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));
    let created_by = created_by_str.and_then(|s| AgentId::from_str(&s).ok());
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(16, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(17, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(Task::restore(RestoreTask {
        id,
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
        notes,
        created_by,
        created_at,
        updated_at,
    }))
}
