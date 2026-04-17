use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use uuid::Uuid;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{
    ApiKey, ApiKeyId, Organization, OrganizationId, OrganizationStore, RestoreOrganization,
};

use crate::SqliteBackend;

impl OrganizationStore for SqliteBackend {
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

        tx.execute(
            "DELETE FROM api_keys WHERE organization_id = ?1",
            rusqlite::params![org.id().to_string()],
        )
        .map_err(|e| Error::Store(e.to_string()))?;

        for key in org.api_keys() {
            tx.execute(
                "INSERT INTO api_keys (id, organization_id, name, key, is_active, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![
                    key.id().to_string(),
                    org.id().to_string(),
                    key.name(),
                    key.key(),
                    key.is_active() as i32,
                    key.created_at().to_rfc3339(),
                ],
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = org.drain_events();
        crate::write_events_in_tx(&tx, &events)?;

        tx.commit().map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let org_row = conn
            .query_row(
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
            .map_err(|e| Error::Store(e.to_string()))?;

        let Some((id_str, name, created_at_str, updated_at_str)) = org_row else {
            return Ok(None);
        };

        let api_keys = load_api_keys(&conn, &id_str)?;
        Ok(Some(build_org(
            id_str,
            name,
            api_keys,
            created_at_str,
            updated_at_str,
        )?))
    }

    async fn find_by_api_key(&self, key: &str) -> Result<Option<Organization>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let row = conn
            .query_row(
                "SELECT o.id, o.name, o.created_at, o.updated_at
                 FROM organizations o
                 JOIN api_keys k ON k.organization_id = o.id
                 WHERE k.key = ?1 AND k.is_active = 1",
                rusqlite::params![key],
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
            .map_err(|e| Error::Store(e.to_string()))?;

        let Some((id_str, name, created_at_str, updated_at_str)) = row else {
            return Ok(None);
        };

        let api_keys = load_api_keys(&conn, &id_str)?;
        Ok(Some(build_org(
            id_str,
            name,
            api_keys,
            created_at_str,
            updated_at_str,
        )?))
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, name, created_at, updated_at FROM organizations ORDER BY created_at",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        let rows: Vec<(String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?;

        rows.into_iter()
            .map(|(id_str, name, created_at_str, updated_at_str)| {
                let api_keys = load_api_keys(&conn, &id_str)?;
                build_org(id_str, name, api_keys, created_at_str, updated_at_str)
            })
            .collect()
    }
}

fn load_api_keys(conn: &rusqlite::Connection, org_id: &str) -> Result<Vec<ApiKey>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, name, key, is_active, created_at FROM api_keys WHERE organization_id = ?1",
        )
        .map_err(|e| Error::Store(e.to_string()))?;

    let keys: Vec<ApiKey> = stmt
        .query_map(rusqlite::params![org_id], |row| {
            let id_str: String = row.get(0)?;
            let name: String = row.get(1)?;
            let key: String = row.get(2)?;
            let is_active: i32 = row.get(3)?;
            let created_at_str: String = row.get(4)?;
            Ok((id_str, name, key, is_active, created_at_str))
        })
        .map_err(|e| Error::Store(e.to_string()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Store(e.to_string()))?
        .into_iter()
        .map(|(id_str, name, key, is_active, created_at_str)| {
            let uuid = Uuid::parse_str(&id_str)
                .map_err(|e| Error::Store(format!("invalid api_keys.id: {e}")))?;
            let created_at = DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| Error::Store(format!("invalid api_keys.created_at: {e}")))?;
            build_api_key(
                ApiKeyId::from_uuid(uuid),
                name,
                key,
                is_active != 0,
                created_at,
            )
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(keys)
}

fn build_api_key(
    id: ApiKeyId,
    name: String,
    key: String,
    is_active: bool,
    created_at: DateTime<Utc>,
) -> Result<ApiKey> {
    use serde_json::json;
    serde_json::from_value(json!({
        "id": id,
        "name": name,
        "key": key,
        "is_active": is_active,
        "created_at": created_at,
    }))
    .map_err(|e| Error::Store(format!("failed to deserialize api keys: {e}")))
}

fn build_org(
    id_str: String,
    name: String,
    api_keys: Vec<ApiKey>,
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
        api_keys,
        created_at,
        updated_at,
    }))
}
