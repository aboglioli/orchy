use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{
    Organization, OrganizationId, OrganizationStore, RestoreOrganization,
};

use crate::SqliteConn;

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
        let mut conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|e| Error::Store(e.to_string()))?;

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
        .map_err(|e| Error::Store(e.to_string()))?;

        let events = org.drain_events();
        crate::events::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

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
        .map_err(|e| Error::Store(e.to_string()))?
        .map(|(id_str, name, created_at_str, updated_at_str)| {
            build_org(id_str, name, created_at_str, updated_at_str)
        })
        .transpose()
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, updated_at FROM organizations ORDER BY created_at",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|e| Error::Store(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Store(e.to_string()))?
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
    let id = OrganizationId::new(&id_str)
        .map_err(|e| Error::Store(format!("invalid organizations.id: {e}")))?;
    let created_at = DateTime::parse_from_rfc3339(&created_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| Error::Store(format!("invalid organizations.created_at: {e}")))?;
    let updated_at = DateTime::parse_from_rfc3339(&updated_at_str)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| Error::Store(format!("invalid organizations.updated_at: {e}")))?;

    Ok(Organization::restore(RestoreOrganization {
        id,
        name,
        created_at,
        updated_at,
    }))
}
