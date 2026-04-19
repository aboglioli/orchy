use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};

use crate::MemoryBackend;

#[async_trait]
impl OrganizationStore for MemoryBackend {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        {
            let mut orgs = self.organizations.write().await;
            orgs.insert(org.id().clone(), org.clone());
        }

        let events = org.drain_events();
        if !events.is_empty() {
            if let Err(e) = orchy_events::io::Writer::write_all(self, &events).await {
                tracing::error!("failed to persist events: {e}");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let orgs = self.organizations.read().await;
        Ok(orgs.get(id).cloned())
    }

    async fn find_by_api_key(&self, key: &str) -> Result<Option<Organization>> {
        let orgs = self.organizations.read().await;
        Ok(orgs
            .values()
            .find(|org| {
                org.api_keys()
                    .iter()
                    .any(|k| k.is_active() && k.key() == key)
            })
            .cloned())
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let orgs = self.organizations.read().await;
        Ok(orgs.values().cloned().collect())
    }
}
