use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};

use crate::dto::OrganizationResponse;

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

    pub async fn execute(&self, cmd: CreateOrganizationCommand) -> Result<OrganizationResponse> {
        let id = OrganizationId::new(&cmd.id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut org = Organization::new(id, cmd.name)?;
        self.orgs.save(&mut org).await?;
        Ok(OrganizationResponse::from(&org))
    }
}
