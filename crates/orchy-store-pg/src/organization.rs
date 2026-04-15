use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{ApiKey, ApiKeyId, Organization, OrganizationId, OrganizationStore, RestoreOrganization};

use crate::PgBackend;

impl OrganizationStore for PgBackend {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        sqlx::query(
            "INSERT INTO organizations (id, name, created_at, updated_at)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(org.id().as_str())
        .bind(org.name())
        .bind(org.created_at())
        .bind(org.updated_at())
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        sqlx::query("DELETE FROM api_keys WHERE organization_id = $1")
            .bind(org.id().as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;

        for key in org.api_keys() {
            sqlx::query(
                "INSERT INTO api_keys (id, organization_id, name, key, is_active, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6)",
            )
            .bind(*key.id().as_uuid())
            .bind(org.id().as_str())
            .bind(key.name())
            .bind(key.key())
            .bind(key.is_active())
            .bind(key.created_at())
            .execute(&self.pool)
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        }

        let events = org.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let row = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM organizations WHERE id = $1",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let org_id_str: String = row.get("id");
        let name: String = row.get("name");
        let created_at: DateTime<Utc> = row.get("created_at");
        let updated_at: DateTime<Utc> = row.get("updated_at");

        let api_keys = load_api_keys_pg(&self.pool, &org_id_str).await?;
        build_org(org_id_str, name, api_keys, created_at, updated_at).map(Some)
    }

    async fn find_by_api_key(&self, key: &str) -> Result<Option<Organization>> {
        let row = sqlx::query(
            "SELECT o.id, o.name, o.created_at, o.updated_at
             FROM organizations o
             JOIN api_keys k ON k.organization_id = o.id
             WHERE k.key = $1 AND k.is_active = true",
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let Some(row) = row else {
            return Ok(None);
        };

        let org_id_str: String = row.get("id");
        let name: String = row.get("name");
        let created_at: DateTime<Utc> = row.get("created_at");
        let updated_at: DateTime<Utc> = row.get("updated_at");

        let api_keys = load_api_keys_pg(&self.pool, &org_id_str).await?;
        build_org(org_id_str, name, api_keys, created_at, updated_at).map(Some)
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let rows = sqlx::query(
            "SELECT id, name, created_at, updated_at FROM organizations ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;

        let mut orgs = Vec::new();
        for row in rows {
            let org_id_str: String = row.get("id");
            let name: String = row.get("name");
            let created_at: DateTime<Utc> = row.get("created_at");
            let updated_at: DateTime<Utc> = row.get("updated_at");
            let api_keys = load_api_keys_pg(&self.pool, &org_id_str).await?;
            orgs.push(build_org(org_id_str, name, api_keys, created_at, updated_at)?);
        }
        Ok(orgs)
    }
}

async fn load_api_keys_pg(pool: &sqlx::PgPool, org_id: &str) -> Result<Vec<ApiKey>> {
    let rows = sqlx::query(
        "SELECT id, name, key, is_active, created_at FROM api_keys WHERE organization_id = $1",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await
    .map_err(|e| Error::Store(e.to_string()))?;

    rows.iter()
        .map(|row| {
            let id: Uuid = row.get("id");
            let name: String = row.get("name");
            let key: String = row.get("key");
            let is_active: bool = row.get("is_active");
            let created_at: DateTime<Utc> = row.get("created_at");
            Ok(build_api_key(ApiKeyId::from_uuid(id), name, key, is_active, created_at))
        })
        .collect()
}

fn build_api_key(id: ApiKeyId, name: String, key: String, is_active: bool, created_at: DateTime<Utc>) -> ApiKey {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "key": key,
        "is_active": is_active,
        "created_at": created_at,
    }))
    .expect("ApiKey deserialization from known-good fields")
}

fn build_org(
    id_str: String,
    name: String,
    api_keys: Vec<ApiKey>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<Organization> {
    let id = OrganizationId::new(&id_str)
        .map_err(|e| Error::Store(format!("invalid organizations.id: {e}")))?;

    Ok(Organization::restore(RestoreOrganization {
        id,
        name,
        api_keys,
        created_at,
        updated_at,
    }))
}
