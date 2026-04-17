use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskId, TaskWatcher, WatcherStore};

use crate::{PgBackend, parse_namespace, parse_project_id};

#[async_trait]
impl WatcherStore for PgBackend {
    async fn save(&self, watcher: &mut TaskWatcher) -> Result<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query(
            "INSERT INTO task_watchers (task_id, agent_id, organization_id, project, namespace, created_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (task_id, agent_id) DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
                project = EXCLUDED.project,
                namespace = EXCLUDED.namespace,
                created_at = EXCLUDED.created_at",
        )
        .bind(watcher.task_id().as_uuid())
        .bind(watcher.agent_id().as_uuid())
        .bind(watcher.org_id().to_string())
        .bind(watcher.project().to_string())
        .bind(watcher.namespace().to_string())
        .bind(watcher.created_at())
        .execute(&mut *tx)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = watcher.drain_events();
        crate::write_events_in_tx(&mut tx, &events).await?;

        tx.commit().await.map_err(|e| Error::Store(e.to_string()))?;
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
            "SELECT task_id, agent_id, organization_id, project, namespace, created_at
             FROM task_watchers WHERE task_id = $1",
        )
        .bind(task_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_watcher).collect()
    }

    async fn find_by_agent(&self, agent_id: &AgentId) -> Result<Vec<TaskWatcher>> {
        let rows = sqlx::query(
            "SELECT task_id, agent_id, organization_id, project, namespace, created_at
             FROM task_watchers WHERE agent_id = $1",
        )
        .bind(agent_id.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        rows.iter().map(row_to_watcher).collect()
    }
}

fn row_to_watcher(row: &sqlx::postgres::PgRow) -> Result<TaskWatcher> {
    let task_id: Uuid = row.get("task_id");
    let agent_id: Uuid = row.get("agent_id");
    let org_id_str: String = row.get("organization_id");
    let project: String = row.get("project");
    let namespace: String = row.get("namespace");
    let created_at: DateTime<Utc> = row.get("created_at");

    Ok(TaskWatcher::restore(
        TaskId::from_uuid(task_id),
        AgentId::from_uuid(agent_id),
        OrganizationId::new(&org_id_str)
            .map_err(|e| Error::Store(format!("invalid task_watchers.organization_id: {e}")))?,
        parse_project_id(project, "task_watchers", "project")?,
        parse_namespace(namespace, "task_watchers", "namespace")?,
        created_at,
    ))
}
