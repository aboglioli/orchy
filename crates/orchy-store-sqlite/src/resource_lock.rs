use std::str::FromStr;

use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock, RestoreResourceLock};

use crate::SqliteBackend;

fn str_err(e: impl ToString) -> Box<dyn std::error::Error + Send + Sync> {
    Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        e.to_string(),
    ))
}

impl LockStore for SqliteBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

        tx.execute(
            "INSERT OR REPLACE INTO resource_locks (organization_id, project, namespace, name, holder, acquired_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                lock.org_id().to_string(),
                lock.project().to_string(),
                lock.namespace().to_string(),
                lock.name(),
                lock.holder().to_string(),
                lock.acquired_at().to_rfc3339(),
                lock.expires_at().to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = lock.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let result = stmt
            .query_row(
                rusqlite::params![
                    org.to_string(),
                    project.to_string(),
                    namespace.to_string(),
                    name
                ],
                row_to_resource_lock,
            )
            .optional()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(result)
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "DELETE FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
            rusqlite::params![
                org.to_string(),
                project.to_string(),
                namespace.to_string(),
                name
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        Ok(())
    }

    async fn find_by_holder(&self, holder: &AgentId) -> Result<Vec<ResourceLock>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE holder = ?1",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let locks = stmt
            .query_map(rusqlite::params![holder.to_string()], row_to_resource_lock)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(locks)
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
    let org_id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let name: String = row.get(3)?;
    let holder_str: String = row.get(4)?;
    let acquired_at_str: String = row.get(5)?;
    let expires_at_str: String = row.get(6)?;

    let org_id = OrganizationId::new(&org_id_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, str_err(e))
    })?;
    let project = ProjectId::try_from(project_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, str_err(e))
    })?;
    let namespace = Namespace::try_from(namespace_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, str_err(e))
    })?;
    let holder = AgentId::from_str(&holder_str).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, str_err(e))
    })?;
    let acquired_at = DateTime::parse_from_rfc3339(&acquired_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, str_err(e))
        })?;
    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, str_err(e))
        })?;

    Ok(ResourceLock::restore(RestoreResourceLock {
        org_id,
        project,
        namespace,
        name,
        holder,
        acquired_at,
        expires_at,
    }))
}
