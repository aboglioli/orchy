use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::api_key::{ApiKey, ApiKeyId, ApiKeyStore, HashedApiKey};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;

use crate::MemoryState;

pub struct MemoryApiKeyStore {
    state: Arc<MemoryState>,
}

impl MemoryApiKeyStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ApiKeyStore for MemoryApiKeyStore {
    async fn save(&self, api_key: &mut ApiKey) -> Result<()> {
        let mut keys = self.state.api_keys.write().await;
        keys.insert(api_key.id().clone(), api_key.clone());
        Ok(())
    }

    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>> {
        let keys = self.state.api_keys.read().await;
        Ok(keys
            .values()
            .find(|k| k.hashed_key() == hash && k.is_active())
            .cloned())
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>> {
        let keys = self.state.api_keys.read().await;
        Ok(keys
            .values()
            .filter(|k| k.org_id() == org_id)
            .cloned()
            .collect())
    }

    async fn revoke(&self, id: &ApiKeyId) -> Result<()> {
        let mut keys = self.state.api_keys.write().await;
        match keys.get_mut(id) {
            Some(key) => {
                key.revoke();
                Ok(())
            }
            None => Err(Error::NotFound(format!("api key {id}"))),
        }
    }
}
