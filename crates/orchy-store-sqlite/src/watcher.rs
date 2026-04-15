use std::str::FromStr;

use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskId, TaskWatcher, WatcherStore};

use crate::SqliteBackend;

impl WatcherStore for SqliteBackend {
    async fn save(&self, watcher: &mut TaskWatcher) -> Result<()> {
        {
            let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
            conn.execute(
                "INSERT OR REPLACE INTO task_watchers (task_id, agent_id, project, namespace, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    watcher.task_id().to_string(),
                    watcher.agent_id().to_string(),
                    watcher.project().to_string(),
                    watcher.namespace().to_string(),
                    watcher.created_at().to_rfc3339(),
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = watcher.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn delete(&self, task_id: &TaskId, agent_id: &AgentId) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM task_watchers WHERE task_id = ?1 AND agent_id = ?2",
            rusqlite::params![task_id.to_string(), agent_id.to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_watchers(&self, task_id: &TaskId) -> Result<Vec<TaskWatcher>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT task_id, agent_id, project, namespace, created_at
                 FROM task_watchers WHERE task_id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let watchers = stmt
            .query_map(rusqlite::params![task_id.to_string()], row_to_watcher)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(watchers)
    }

    async fn find_by_agent(&self, agent_id: &AgentId) -> Result<Vec<TaskWatcher>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT task_id, agent_id, project, namespace, created_at
                 FROM task_watchers WHERE agent_id = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let watchers = stmt
            .query_map(rusqlite::params![agent_id.to_string()], row_to_watcher)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(watchers)
    }
}

fn row_to_watcher(row: &rusqlite::Row) -> rusqlite::Result<TaskWatcher> {
    let task_id_str: String = row.get(0)?;
    let agent_id_str: String = row.get(1)?;
    let project_str: String = row.get(2)?;
    let namespace_str: String = row.get(3)?;
    let created_at_str: String = row.get(4)?;

    let task_id = TaskId::from_str(&task_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let agent_id = AgentId::from_str(&agent_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            2,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            3,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(TaskWatcher::restore(
        task_id,
        agent_id,
        OrganizationId::new("default").unwrap(),
        project,
        namespace,
        created_at,
    ))
}
