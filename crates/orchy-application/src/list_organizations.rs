use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;

use crate::dto::OrganizationResponse;

pub struct ListOrganizations {
    orgs: Arc<dyn OrganizationStore>,
}

impl ListOrganizations {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self) -> Result<Vec<OrganizationResponse>> {
        let orgs = self.orgs.list().await?;
        Ok(orgs.iter().map(OrganizationResponse::from).collect())
    }
}
