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
        self.state
            .organizations
            .insert(org.id().clone(), org.clone());

        let events = org.drain_events();
        self.state.append_events(events).await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        Ok(self.state.organizations.get(id).map(|r| r.clone()))
    }

    async fn list(&self) -> Result<Vec<Organization>> {
        Ok(self
            .state
            .organizations
            .iter()
            .map(|e| e.value().clone())
            .collect())
    }
}
