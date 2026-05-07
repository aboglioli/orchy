use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::organization::{OrganizationId, OrganizationStore};

use crate::dto::OrganizationDto;

pub struct GetOrganizationCommand {
    pub id: String,
}

pub struct GetOrganization {
    orgs: Arc<dyn OrganizationStore>,
}

impl GetOrganization {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, cmd: GetOrganizationCommand) -> ApplicationResult<OrganizationDto> {
        let id = OrganizationId::new(&cmd.id)?;
        let org = self
            .orgs
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Organization,
                id: id.to_string(),
            })?;
        Ok(OrganizationDto::from(&org))
    }
}
