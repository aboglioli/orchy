use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use orchy_core::api_key::{
    ApiKey, ApiKeyId, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, RestoreApiKey,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::UserId;

pub struct PgApiKeyStore {
    pool: PgPool,
}

impl PgApiKeyStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ApiKeyStore for PgApiKeyStore {
    async fn save(&self, api_key: &mut ApiKey) -> Result<()> {
        sqlx::query(
            "INSERT INTO api_keys (id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                is_active = EXCLUDED.is_active",
        )
        .bind(*api_key.id().as_uuid())
        .bind(api_key.org_id().as_str())
        .bind(api_key.name())
        .bind(api_key.hashed_key().as_str())
        .bind(api_key.key_prefix().as_str())
        .bind(api_key.key_suffix().as_str())
        .bind(api_key.is_active())
        .bind(api_key.created_at())
        .bind(api_key.user_id().map(|u| u.as_uuid()))
        .execute(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ApiKeyId) -> Result<Option<ApiKey>> {
        sqlx::query(
            "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
             FROM api_keys WHERE id = $1",
        )
        .bind(*id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?
        .map(|r| build_api_key(&r))
        .transpose()
    }

    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>> {
        sqlx::query(
            "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
             FROM api_keys WHERE key_hash = $1",
        )
        .bind(hash.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?
        .map(|r| build_api_key(&r))
        .transpose()
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>> {
        sqlx::query(
            "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
             FROM api_keys WHERE organization_id = $1 ORDER BY created_at",
        )
        .bind(org_id.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| Error::Store(e.to_string()))?
        .iter()
        .map(build_api_key)
        .collect()
    }
}

fn build_api_key(row: &sqlx::postgres::PgRow) -> Result<ApiKey> {
    let id: Uuid = row.get("id");
    let org_id_str: String = row.get("organization_id");
    let name: String = row.get("name");
    let key_hash: String = row.get("key_hash");
    let key_prefix: String = row.get("key_prefix");
    let key_suffix: String = row.get("key_suffix");
    let is_active: bool = row.get("is_active");
    let created_at: DateTime<Utc> = row.get("created_at");
    let user_id_uuid: Option<Uuid> = row.try_get("user_id").ok().flatten();

    Ok(ApiKey::restore(RestoreApiKey {
        id: ApiKeyId::from_uuid(id),
        org_id: OrganizationId::new(&org_id_str).map_err(|e| Error::Store(e.to_string()))?,
        name,
        hashed_key: HashedApiKey::new(key_hash)?,
        key_prefix: ApiKeyPrefix::new(key_prefix)?,
        key_suffix: ApiKeySuffix::new(key_suffix)?,
        user_id: user_id_uuid.map(UserId::from_uuid),
        is_active,
        created_at,
    }))
}
