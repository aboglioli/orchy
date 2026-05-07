use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::api_key::{ApiKeyGenerator, ApiKeyStore};
use orchy_core::error::Error;
use orchy_core::organization::OrganizationId;
use orchy_core::user::UserId;
use serde::Serialize;
use std::str::FromStr;

pub struct GenerateApiKeyCommand {
    pub org_id: String,
    pub user_id: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenerateApiKeyResponse {
    pub api_key: String,
}

pub struct GenerateApiKey {
    api_keys: Arc<dyn ApiKeyStore>,
    generator: Arc<dyn ApiKeyGenerator>,
}

impl GenerateApiKey {
    pub fn new(api_keys: Arc<dyn ApiKeyStore>, generator: Arc<dyn ApiKeyGenerator>) -> Self {
        Self {
            api_keys,
            generator,
        }
    }

    pub async fn execute(
        &self,
        cmd: GenerateApiKeyCommand,
    ) -> ApplicationResult<GenerateApiKeyResponse> {
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let user_id = cmd
            .user_id
            .as_deref()
            .map(UserId::from_str)
            .transpose()
            .map_err(|e| Error::invalid_input(format!("invalid user_id: {e}")))?;

        let (plain, mut api_key) = self.generator.generate(&org_id, user_id, cmd.name)?;
        self.api_keys.save(&mut api_key).await?;

        Ok(GenerateApiKeyResponse {
            api_key: plain.as_str().to_string(),
        })
    }
}
