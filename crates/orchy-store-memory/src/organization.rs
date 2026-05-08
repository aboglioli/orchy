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
        self.state.append_events(events).await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        let orgs = self.state.organizations.read().await;
        Ok(orgs.get(id).cloned())
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        let orgs = self.state.organizations.read().await;
        Ok(orgs.values().cloned().collect())
    }
}
