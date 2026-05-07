use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};

use crate::dto::OrganizationDto;

pub struct CreateOrganizationCommand {
    pub id: String,
    pub name: String,
}

pub struct CreateOrganization {
    orgs: Arc<dyn OrganizationStore>,
}

impl CreateOrganization {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(
        &self,
        cmd: CreateOrganizationCommand,
    ) -> ApplicationResult<OrganizationDto> {
        let id = OrganizationId::new(&cmd.id)?;
        let mut org = Organization::new(id, cmd.name)?;
        self.orgs.save(&mut org).await?;
        Ok(OrganizationDto::from(&org))
    }
}
