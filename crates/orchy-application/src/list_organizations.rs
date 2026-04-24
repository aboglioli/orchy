use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;

use crate::dto::OrganizationDto;

pub struct ListOrganizationsCommand {}

pub struct ListOrganizations {
    orgs: Arc<dyn OrganizationStore>,
}

impl ListOrganizations {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, _cmd: ListOrganizationsCommand) -> Result<Vec<OrganizationDto>> {
        let orgs = self.orgs.list().await?;
        Ok(orgs.iter().map(OrganizationDto::from).collect())
    }
}
