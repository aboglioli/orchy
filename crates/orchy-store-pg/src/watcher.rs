use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::task::{TaskId, TaskWatcher, WatcherStore};

use crate::PgBackend;

impl WatcherStore for PgBackend {
    async fn save(&self, watcher: &mut TaskWatcher) -> Result<()> {
        sqlx::query(
            "INSERT INTO task_watchers (task_id, agent_id, project, namespace, created_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (task_id, agent_id) DO UPDATE SET
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                created_at = EXCLUDED.created_at",
        )
        .bind(watcher.task_id().as_uuid())
        .bind(watcher.agent_id().as_uuid())
        .bind(watcher.project().to_string())
        .bind(watcher.namespace().to_string())
        .bind(watcher.created_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = watcher.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn delete(&self, task_id: &TaskId, agent_id: &AgentId) -> Result<()> {
        sqlx::query("DELETE FROM task_watchers WHERE task_id = $1 AND agent_id = $2")
            .bind(task_id.as_uuid())
            .bind(agent_id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_watchers(&self, task_id: &TaskId) -> Result<Vec<TaskWatcher>> {
        let rows = sqlx::query(
            "SELECT task_id, agent_id, project, namespace, created_at
             FROM task_watchers WHERE task_id = $1",
        )
        .bind(task_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_watcher).collect())
    }

    async fn find_by_agent(&self, agent_id: &AgentId) -> Result<Vec<TaskWatcher>> {
        let rows = sqlx::query(
            "SELECT task_id, agent_id, project, namespace, created_at
             FROM task_watchers WHERE agent_id = $1",
        )
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(rows.iter().map(row_to_watcher).collect())
    }
}

fn row_to_watcher(row: &sqlx::postgres::PgRow) -> TaskWatcher {
    let task_id: Uuid = row.get("task_id");
    let agent_id: Uuid = row.get("agent_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let created_at: DateTime<Utc> = row.get("created_at");

    TaskWatcher::restore(
        TaskId::from_uuid(task_id),
        AgentId::from_uuid(agent_id),
        ProjectId::try_from(project).expect("invalid project in database"),
        Namespace::try_from(namespace).unwrap(),
        created_at,
    )
}
