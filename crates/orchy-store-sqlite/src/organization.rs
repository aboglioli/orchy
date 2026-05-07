use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use std::result::Result as StdResult;

use orchy_core::error::{Error, Result, StoreError};
use orchy_core::organization::{
    Organization, OrganizationId, OrganizationStore, RestoreOrganization,
};

use crate::{SqliteConn, events};

pub struct SqliteOrganizationStore {
    conn: SqliteConn,
}

impl SqliteOrganizationStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl OrganizationStore for SqliteOrganizationStore {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        let mut conn = self.conn.lock().map_err(crate::error::lock_err)?;
        let tx = conn.transaction().map_err(crate::error::store_err)?;

        tx.execute(
            "INSERT OR REPLACE INTO organizations (id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                org.id().to_string(),
                org.name(),
                org.created_at().to_rfc3339(),
                org.updated_at().to_rfc3339(),
            ],
        )
        .map_err(crate::error::store_err)?;

        let events = org.drain_events();
        events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(crate::error::store_err)?;
        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

        conn.query_row(
            "SELECT id, name, created_at, updated_at FROM organizations WHERE id = ?1",
            rusqlite::params![id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(crate::error::store_err)?
        .map(|(id_str, name, created_at_str, updated_at_str)| {
            build_org(id_str, name, created_at_str, updated_at_str)
        })
        .transpose()
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let conn = self.conn.lock().map_err(crate::error::lock_err)?;

        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, updated_at FROM organizations ORDER BY created_at",
            )
            .map_err(crate::error::store_err)?;

        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(crate::error::store_err)?
        .collect::<StdResult<Vec<_>, _>>()
        .map_err(crate::error::store_err)?
        .into_iter()
        .map(|(id_str, name, created_at_str, updated_at_str)| {
            build_org(id_str, name, created_at_str, updated_at_str)
        })
        .collect()
    }
}

fn build_org(
    id_str: String,
    name: String,
    created_at_str: String,
    updated_at_str: String,
) -> Result<Organization> {
    let id = OrganizationId::new(&id_str).map_err(|e| {
        Error::Store(StoreError::Decode {
            table: "organizations".to_string(),
            column: "id".to_string(),
            cause: e.to_string(),
        })
    })?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            Error::Store(StoreError::Other(format!(
                "invalid organizations.created_at: {e}"
            )))
        })?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| {
            Error::Store(StoreError::Other(format!(
                "invalid organizations.updated_at: {e}"
            )))
        })?;

    Ok(Organization::restore(RestoreOrganization {
        id,
        name,
        created_at,
        updated_at,
    }))
}
