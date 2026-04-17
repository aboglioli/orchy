use std::sync::Arc;

use crate::error::{Error, Result};

use super::{ApiKeyId, Organization, OrganizationId, OrganizationStore};

pub struct OrganizationService<S: OrganizationStore> {
    store: Arc<S>,
}

impl<S: OrganizationStore> OrganizationService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn create(&self, id: OrganizationId, name: String) -> Result<Organization> {
        let mut org = Organization::new(id, name)?;
        self.store.save(&mut org).await?;
        Ok(org)
    }

    pub async fn get(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        self.store.find_by_id(id).await
    }

    pub async fn list(&self) -> Result<Vec<Organization>> {
        self.store.list().await
    }

    pub async fn resolve_api_key(&self, key: &str) -> Result<Option<Organization>> {
        self.store.find_by_api_key(key).await
    }

    pub async fn add_api_key(
        &self,
        org_id: &OrganizationId,
        name: String,
        key: String,
    ) -> Result<Organization> {
        let mut org = self
            .store
            .find_by_id(org_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("organization {org_id}")))?;
        org.add_api_key(name, key)?;
        self.store.save(&mut org).await?;
        Ok(org)
    }

    pub async fn revoke_api_key(
        &self,
        org_id: &OrganizationId,
        key_id: &ApiKeyId,
    ) -> Result<Organization> {
        let mut org = self
            .store
            .find_by_id(org_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("organization {org_id}")))?;
        org.revoke_api_key(key_id)?;
        self.store.save(&mut org).await?;
        Ok(org)
    }
}
