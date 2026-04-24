use std::str::FromStr;
use std::sync::Arc;

use orchy_core::api_key::ApiKeyGenerator;
use orchy_core::error::{Error, Result};
use orchy_core::organization::{OrganizationId, OrganizationStore};
use orchy_core::user::UserId;
use serde::Serialize;

pub struct GenerateApiKeyCommand {
    pub org_id: String,
    pub name: String,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateApiKeyResponse {
    pub api_key: String,
}

pub struct GenerateApiKey {
    orgs: Arc<dyn OrganizationStore>,
    generator: Arc<dyn ApiKeyGenerator>,
}

impl GenerateApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>, generator: Arc<dyn ApiKeyGenerator>) -> Self {
        Self { orgs, generator }
    }

    pub async fn execute(&self, cmd: GenerateApiKeyCommand) -> Result<GenerateApiKeyResponse> {
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

        let raw_key = self.generator.generate();
        let key_hash = self.generator.hash(&raw_key)?;
        let key_prefix = self.generator.extract_prefix(&raw_key)?;

        let key_suffix = raw_key.suffix().to_string();
        org.add_api_key(cmd.name, key_hash, key_prefix, key_suffix, user_id)?;
        self.orgs.save(&mut org).await?;

        Ok(GenerateApiKeyResponse {
            api_key: raw_key.as_str().to_string(),
        })
    }
}
