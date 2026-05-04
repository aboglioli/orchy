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
        let events = api_key.drain_events();
        let mut keys = self.state.api_keys.write().await;
        keys.insert(api_key.id().clone(), api_key.clone());
        if !events.is_empty() {
            let mut events_guard = self.state.events.write().await;
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| Error::Store(e.to_string()))?;
                events_guard.push(serialized);
            }
        }
        Ok(())
    }

    async fn find_by_id(&self, id: &ApiKeyId) -> Result<Option<ApiKey>> {
        let keys = self.state.api_keys.read().await;
        Ok(keys.get(id).cloned())
    }

    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>> {
        let keys = self.state.api_keys.read().await;
        Ok(keys.values().find(|k| k.hashed_key() == hash).cloned())
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>> {
        let keys = self.state.api_keys.read().await;
        Ok(keys
            .values()
            .filter(|k| k.org_id() == org_id)
            .cloned()
            .collect())
    }
}
