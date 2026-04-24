use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};

use crate::MemoryState;

pub struct MemoryOrganizationStore {
    state: Arc<MemoryState>,
}

impl MemoryOrganizationStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl OrganizationStore for MemoryOrganizationStore {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        {
            let mut orgs = self.state.organizations.write().await;
            orgs.insert(org.id().clone(), org.clone());
        }

        let events = org.drain_events();
        if !events.is_empty() {
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| orchy_core::error::Error::Store(e.to_string()))?;
                self.state.events.write().await.push(serialized);
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let orgs = self.state.organizations.read().await;
        Ok(orgs.get(id).cloned())
    }

    async fn find_by_api_key_hash(&self, key_hash: &str) -> Result<Option<Organization>> {
        let orgs = self.state.organizations.read().await;
        Ok(orgs
            .values()
            .find(|org| {
                org.api_keys()
                    .iter()
                    .any(|k| k.is_active() && k.key_hash().as_str() == key_hash)
            })
            .cloned())
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let orgs = self.state.organizations.read().await;
        Ok(orgs.values().cloned().collect())
    }
}
