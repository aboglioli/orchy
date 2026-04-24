use std::sync::Arc;

use orchy_core::error::{Error, Result};
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

    pub async fn execute(&self, cmd: GetOrganizationCommand) -> Result<OrganizationDto> {
        let id = OrganizationId::new(&cmd.id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let org = self
            .orgs
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("organization {id}")))?;
        Ok(OrganizationDto::from(&org))
    }
}
