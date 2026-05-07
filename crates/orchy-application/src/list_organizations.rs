use std::sync::Arc;

use crate::error::ApplicationResult;
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

    pub async fn execute(
        &self,
        _cmd: ListOrganizationsCommand,
    ) -> ApplicationResult<Vec<OrganizationDto>> {
        let orgs = self.orgs.list().await?;
        Ok(orgs.iter().map(OrganizationDto::from).collect())
    }
}
