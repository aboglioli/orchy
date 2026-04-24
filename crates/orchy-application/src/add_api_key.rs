use std::str::FromStr;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::organization::{OrganizationId, OrganizationStore};
use orchy_core::user::UserId;

use crate::dto::OrganizationDto;

pub struct AddApiKeyCommand {
    pub org_id: String,
    pub name: String,
    pub key: String,
    pub user_id: Option<String>,
}

pub struct AddApiKey {
    orgs: Arc<dyn OrganizationStore>,
}

impl AddApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, cmd: AddApiKeyCommand) -> Result<OrganizationDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut org = self
            .orgs
            .find_by_id(&org_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("organization {org_id}")))?;
        let user_id = cmd
            .user_id
            .as_deref()
            .map(UserId::from_str)
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid user_id: {e}")))?;
        org.add_api_key(cmd.name, cmd.key, user_id)?;
        self.orgs.save(&mut org).await?;
        Ok(OrganizationDto::from(&org))
    }
}
