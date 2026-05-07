use rusqlite::{Error as RusqliteError, Row as RusqliteRow, types::Type as RusqliteType};
use std::error::Error as StdError;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::result::Result as StdResult;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rusqlite::OptionalExtension;

use orchy_core::agent::AgentId;
use orchy_core::error::Result;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock, RestoreResourceLock};

use crate::{SqliteConn, events};

fn str_err(e: impl ToString) -> Box<dyn StdError + Send + Sync> {
    Box::new(IoError::new(IoErrorKind::InvalidData, e.to_string()))
}

pub struct SqliteLockStore {
    conn: SqliteConn,
}

impl SqliteLockStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl LockStore for SqliteLockStore {
    async fn acquire_if_free(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
        holder: &AgentId,
        ttl_secs: u64,
    ) -> Result<Option<ResourceLock>> {
        let now = Utc::now();
        let expires_at = now + Duration::seconds(ttl_secs as i64);

        let mut conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let tx = conn.transaction().map_err(crate::error::store_err)?;

        let affected = tx
            .execute(
                "INSERT INTO resource_locks (organization_id, project, namespace, name, holder, acquired_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT (organization_id, project, namespace, name) DO UPDATE
                 SET holder = excluded.holder, acquired_at = excluded.acquired_at, expires_at = excluded.expires_at
                 WHERE resource_locks.expires_at <= ?6 OR resource_locks.holder = excluded.holder",
                rusqlite::params![
                    org.to_string(),
                    project.to_string(),
                    namespace.to_string(),
                    name,
                    holder.to_string(),
                    now.to_rfc3339(),
                    expires_at.to_rfc3339(),
                ],
            )
            .map_err(crate::error::store_err)?;

        if affected == 0 {
            return Ok(None);
        }

        let mut lock = ResourceLock::acquire(
            org.clone(),
            project.clone(),
            namespace.clone(),
            name.to_string(),
            holder.clone(),
            ttl_secs,
        )?;
        let events = lock.drain_events();
        events::write_events_in_tx(&tx, &events)?;
        tx.commit().map_err(crate::error::store_err)?;
        Ok(Some(lock))
    }

    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        let mut conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let tx = conn.transaction().map_err(crate::error::store_err)?;

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
        .map_err(crate::error::store_err)?;

        let events = lock.drain_events();
        events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let mut stmt = conn
            .prepare(
                "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
            )
            .map_err(crate::error::store_err)?;

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
            .map_err(crate::error::store_err)?;

        Ok(result)
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        conn.execute(
            "DELETE FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
            rusqlite::params![
                org.to_string(),
                project.to_string(),
                namespace.to_string(),
                name
            ],
        )
        .map_err(crate::error::store_err)?;

        Ok(())
    }

    async fn find_by_holder(
        &self,
        holder: &AgentId,
        org: &OrganizationId,
    ) -> Result<Vec<ResourceLock>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let mut stmt = conn
            .prepare(
                "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE holder = ?1 AND organization_id = ?2",
            )
            .map_err(crate::error::store_err)?;

        let locks = stmt
            .query_map(
                rusqlite::params![holder.to_string(), org.to_string()],
                row_to_resource_lock,
            )
            .map_err(crate::error::store_err)?
            .collect::<StdResult<Vec<_>, _>>()
            .map_err(crate::error::store_err)?;

        Ok(locks)
    }

    async fn release_for_agent(&self, holder: &AgentId, org: &OrganizationId) -> Result<u64> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let count = conn
            .execute(
                "DELETE FROM resource_locks WHERE holder = ?1 AND organization_id = ?2",
                rusqlite::params![holder.to_string(), org.to_string()],
            )
            .map_err(crate::error::store_err)?;
        Ok(count as u64)
    }

    async fn delete_expired(&self) -> Result<u64> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let now = Utc::now().to_rfc3339();
        let count = conn
            .execute(
                "DELETE FROM resource_locks WHERE expires_at < ?1",
                rusqlite::params![now],
            )
            .map_err(crate::error::store_err)?;

        Ok(count as u64)
    }
}

fn row_to_resource_lock(row: &RusqliteRow) -> rusqlite::Result<ResourceLock> {
    let org_id_str: String = row.get(0)?;
    let project_str: String = row.get(1)?;
    let namespace_str: String = row.get(2)?;
    let name: String = row.get(3)?;
    let holder_str: String = row.get(4)?;
    let acquired_at_str: String = row.get(5)?;
    let expires_at_str: String = row.get(6)?;

    let org_id = OrganizationId::new(&org_id_str)
        .map_err(|e| RusqliteError::FromSqlConversionFailure(0, RusqliteType::Text, str_err(e)))?;
    let project = ProjectId::try_from(project_str)
        .map_err(|e| RusqliteError::FromSqlConversionFailure(1, RusqliteType::Text, str_err(e)))?;
    let namespace = Namespace::try_from(namespace_str)
        .map_err(|e| RusqliteError::FromSqlConversionFailure(2, RusqliteType::Text, str_err(e)))?;
    let holder = AgentId::from_str(&holder_str)
        .map_err(|e| RusqliteError::FromSqlConversionFailure(4, RusqliteType::Text, str_err(e)))?;
    let acquired_at = DateTime::parse_from_rfc3339(&acquired_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RusqliteError::FromSqlConversionFailure(5, RusqliteType::Text, str_err(e)))?;
    let expires_at = DateTime::parse_from_rfc3339(&expires_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| RusqliteError::FromSqlConversionFailure(6, RusqliteType::Text, str_err(e)))?;

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
