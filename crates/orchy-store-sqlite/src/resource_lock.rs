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

use crate::{SqliteConn, blocking, blocking_tx, events};

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
        let org_owned = org.clone();
        let project_owned = project.clone();
        let namespace_owned = namespace.clone();
        let name_owned = name.to_owned();
        let holder_owned = holder.clone();

        let lock_opt = blocking_tx(&self.conn, move |tx| {
            let affected = tx
                .execute(
                    "INSERT INTO resource_locks (organization_id, project, namespace, name, holder, acquired_at, expires_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                     ON CONFLICT (organization_id, project, namespace, name) DO UPDATE
                     SET holder = excluded.holder, acquired_at = excluded.acquired_at, expires_at = excluded.expires_at
                     WHERE resource_locks.expires_at <= ?6 OR resource_locks.holder = excluded.holder",
                    rusqlite::params![
                        org_owned.to_string(),
                        project_owned.to_string(),
                        namespace_owned.to_string(),
                        name_owned,
                        holder_owned.to_string(),
                        now.to_rfc3339(),
                        expires_at.to_rfc3339(),
                    ],
                )
                .map_err(crate::error::store_err)?;

            if affected == 0 {
                return Ok(None);
            }

            let mut lock = ResourceLock::acquire(
                org_owned,
                project_owned,
                namespace_owned,
                name_owned,
                holder_owned,
                ttl_secs,
            )?;
            let events = lock.drain_events();
            events::write_events_in_tx(tx, &events)?;
            Ok(Some(lock))
        })
        .await?;
        Ok(lock_opt)
    }

    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        let org_id = lock.org_id().to_string();
        let project = lock.project().to_string();
        let namespace = lock.namespace().to_string();
        let name = lock.name().to_owned();
        let holder = lock.holder().to_string();
        let acquired_at = lock.acquired_at().to_rfc3339();
        let expires_at = lock.expires_at().to_rfc3339();
        let drained = lock.drain_events();
        blocking_tx(&self.conn, move |tx| {
            tx.execute(
                "INSERT OR REPLACE INTO resource_locks (organization_id, project, namespace, name, holder, acquired_at, expires_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![org_id, project, namespace, name, holder, acquired_at, expires_at],
            )
            .map_err(crate::error::store_err)?;
            events::write_events_in_tx(tx, &drained)?;
            Ok(())
        })
        .await
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let org = org.to_string();
        let project = project.to_string();
        let namespace = namespace.to_string();
        let name = name.to_owned();
        blocking(&self.conn, move |conn| {
            conn.query_row(
                "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                 FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
                rusqlite::params![org, project, namespace, name],
                row_to_resource_lock,
            )
            .optional()
            .map_err(crate::error::store_err)
        })
        .await
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let org = org.to_string();
        let project = project.to_string();
        let namespace = namespace.to_string();
        let name = name.to_owned();
        blocking(&self.conn, move |conn| {
            conn.execute(
                "DELETE FROM resource_locks WHERE organization_id = ?1 AND project = ?2 AND namespace = ?3 AND name = ?4",
                rusqlite::params![org, project, namespace, name],
            )
            .map_err(crate::error::store_err)?;
            Ok(())
        })
        .await
    }

    async fn find_by_holder(
        &self,
        holder: &AgentId,
        org: &OrganizationId,
    ) -> Result<Vec<ResourceLock>> {
        let holder = holder.to_string();
        let org = org.to_string();
        blocking(&self.conn, move |conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT organization_id, project, namespace, name, holder, acquired_at, expires_at
                     FROM resource_locks WHERE holder = ?1 AND organization_id = ?2",
                )
                .map_err(crate::error::store_err)?;

            stmt.query_map(rusqlite::params![holder, org], row_to_resource_lock)
                .map_err(crate::error::store_err)?
                .collect::<StdResult<Vec<_>, _>>()
                .map_err(crate::error::store_err)
        })
        .await
    }

    async fn release_for_agent(&self, holder: &AgentId, org: &OrganizationId) -> Result<u64> {
        let holder = holder.to_string();
        let org = org.to_string();
        blocking(&self.conn, move |conn| {
            let count = conn
                .execute(
                    "DELETE FROM resource_locks WHERE holder = ?1 AND organization_id = ?2",
                    rusqlite::params![holder, org],
                )
                .map_err(crate::error::store_err)?;
            Ok(count as u64)
        })
        .await
    }

    async fn delete_expired(&self) -> Result<u64> {
        let now = Utc::now().to_rfc3339();
        blocking(&self.conn, move |conn| {
            let count = conn
                .execute(
                    "DELETE FROM resource_locks WHERE expires_at < ?1",
                    rusqlite::params![now],
                )
                .map_err(crate::error::store_err)?;
            Ok(count as u64)
        })
        .await
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
