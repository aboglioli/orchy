use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::resource_lock::{LockStore, ResourceLock, RestoreResourceLock};

use crate::SqliteBackend;

impl LockStore for SqliteBackend {
    async fn save(&self, lock: &ResourceLock) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO resource_locks (project, namespace, name, holder, acquired_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                lock.project().to_string(),
                lock.namespace().to_string(),
                lock.name(),
                lock.holder().to_string(),
                lock.acquired_at().to_rfc3339(),
                lock.expires_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE project = ?1 AND namespace = ?2 AND name = ?3",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![project.to_string(), namespace.to_string(), name],
                row_to_resource_lock,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM resource_locks WHERE project = ?1 AND namespace = ?2 AND name = ?3",
            rusqlite::params![project.to_string(), namespace.to_string(), name],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn delete_expired(&self) -> Result<u64> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let now = Utc::now().to_rfc3339();
        let count = conn
            .execute(
                "DELETE FROM resource_locks WHERE expires_at < ?1",
                rusqlite::params![now],
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(count as u64)
    }
}

fn row_to_resource_lock(row: &rusqlite::Row) -> rusqlite::Result<ResourceLock> {
    let project_str: String = row.get(0)?;
    let namespace_str: String = row.get(1)?;
    let name: String = row.get(2)?;
    let holder_str: String = row.get(3)?;
    let acquired_at_str: String = row.get(4)?;
    let expires_at_str: String = row.get(5)?;

    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e)),
        )
    })?;
    let holder = AgentId::from_str(&holder_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let acquired_at = DateTime::parse_from_rfc3339(&acquired_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?;
    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;

    Ok(ResourceLock::restore(RestoreResourceLock {
        project,
        namespace,
        name,
        holder,
        acquired_at,
        expires_at,
    }))
}
