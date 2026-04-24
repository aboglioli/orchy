use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{ApiKeyId, OrganizationId, OrganizationStore};

use crate::dto::OrganizationDto;

pub struct RevokeApiKeyCommand {
    pub org_id: String,
    pub key_id: String,
}

pub struct RevokeApiKey {
    orgs: Arc<dyn OrganizationStore>,
}

impl RevokeApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, cmd: RevokeApiKeyCommand) -> Result<OrganizationDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let key_uuid: uuid::Uuid = cmd
            .key_id
            .parse()
            .map_err(|e: uuid::Error| Error::InvalidInput(e.to_string()))?;
        let key_id = ApiKeyId::from_uuid(key_uuid);

        let mut org = self
            .orgs
            .find_by_id(&org_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("organization {org_id}")))?;
        org.revoke_api_key(&key_id)?;
        self.orgs.save(&mut org).await?;
        Ok(OrganizationDto::from(&org))
    }
}
