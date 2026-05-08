use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::api_key::{ApiKey, ApiKeyId, ApiKeyStore, HashedApiKey};
use orchy_core::error::Result;
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
        let events = api_key.drain_events();
        self.state
            .api_keys
            .insert(api_key.id().clone(), api_key.clone());
        self.state.append_events(events).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &ApiKeyId) -> Result<Option<ApiKey>> {
        Ok(self.state.api_keys.get(id).map(|r| r.clone()))
    }

    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>> {
        Ok(self.state.api_keys.iter().find_map(|entry| {
            if entry.value().hashed_key() == hash {
                Some(entry.value().clone())
            } else {
                None
            }
        }))
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>> {
        Ok(self
            .state
            .api_keys
            .iter()
            .filter(|e| e.value().org_id() == org_id)
            .map(|e| e.value().clone())
            .collect())
    }
}
