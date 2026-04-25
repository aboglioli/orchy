use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::OptionalExtension;
use uuid::Uuid;

use orchy_core::api_key::{
    ApiKey, ApiKeyId, ApiKeyPrefix, ApiKeyStore, ApiKeySuffix, HashedApiKey, RestoreApiKey,
};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::UserId;

use crate::SqliteConn;

pub struct SqliteApiKeyStore {
    conn: SqliteConn,
}

impl SqliteApiKeyStore {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl ApiKeyStore for SqliteApiKeyStore {
    async fn save(&self, api_key: &mut ApiKey) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO api_keys (id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                api_key.id().to_string(),
                api_key.org_id().to_string(),
                api_key.name(),
                api_key.hashed_key().as_str(),
                api_key.key_prefix().as_str(),
                api_key.key_suffix().as_str(),
                api_key.is_active() as i32,
                api_key.created_at().to_rfc3339(),
                api_key.user_id().map(|u| u.to_string()),
            ],
        )
        .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ApiKeyId) -> Result<Option<ApiKey>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.query_row(
            "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
             FROM api_keys WHERE id = ?1",
            rusqlite::params![id.to_string()],
            row_to_tuple,
        )
        .optional()
        .map_err(|e| Error::Store(e.to_string()))?
        .map(build_api_key)
        .transpose()
    }

    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        conn.query_row(
            "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
             FROM api_keys WHERE key_hash = ?1",
            rusqlite::params![hash.as_str()],
            row_to_tuple,
        )
        .optional()
        .map_err(|e| Error::Store(e.to_string()))?
        .map(build_api_key)
        .transpose()
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>> {
        let conn = self.conn.lock().map_err(|e| Error::Store(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, organization_id, name, key_hash, key_prefix, key_suffix, is_active, created_at, user_id
                 FROM api_keys WHERE organization_id = ?1 ORDER BY created_at",
            )
            .map_err(|e| Error::Store(e.to_string()))?;

        stmt.query_map(rusqlite::params![org_id.to_string()], row_to_tuple)
            .map_err(|e| Error::Store(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::Store(e.to_string()))?
            .into_iter()
            .map(build_api_key)
            .collect()
    }
}

type ApiKeyRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    i32,
    String,
    Option<String>,
);

fn row_to_tuple(row: &rusqlite::Row) -> rusqlite::Result<ApiKeyRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get::<_, Option<String>>(8).ok().flatten(),
    ))
}

fn build_api_key(row: ApiKeyRow) -> Result<ApiKey> {
    let (
        id_str,
        org_id_str,
        name,
        key_hash,
        key_prefix,
        key_suffix,
        is_active,
        created_at_str,
        user_id_str,
    ) = row;

    Ok(ApiKey::restore(RestoreApiKey {
        id: Uuid::parse_str(&id_str)
            .map(ApiKeyId::from_uuid)
            .map_err(|e| Error::Store(format!("invalid api_keys.id: {e}")))?,
        org_id: OrganizationId::new(&org_id_str).map_err(|e| Error::Store(e.to_string()))?,
        name,
        hashed_key: HashedApiKey::new(key_hash)?,
        key_prefix: ApiKeyPrefix::new(key_prefix)?,
        key_suffix: ApiKeySuffix::new(key_suffix)?,
        user_id: user_id_str.and_then(|s| s.parse::<UserId>().ok()),
        is_active: is_active != 0,
        created_at: DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| Error::Store(format!("invalid api_keys.created_at: {e}")))?,
    }))
}
