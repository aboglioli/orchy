use std::str::FromStr;
use std::sync::Arc;

use orchy_core::api_key::{ApiKeyId, ApiKeyStore};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;

pub struct RevokeApiKeyCommand {
    pub key_id: String,
    pub org_id: String,
}

pub struct RevokeApiKey {
    api_keys: Arc<dyn ApiKeyStore>,
}

impl RevokeApiKey {
    pub fn new(api_keys: Arc<dyn ApiKeyStore>) -> Self {
        Self { api_keys }
    }

    pub async fn execute(&self, cmd: RevokeApiKeyCommand) -> Result<()> {
        let key_id =
            ApiKeyId::from_str(&cmd.key_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut api_key = self
            .api_keys
            .find_by_id(&key_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("api key {key_id}")))?;

        if api_key.org_id() != &org_id {
            return Err(Error::NotFound(format!("api key {key_id}")));
        }

        api_key.revoke();
        self.api_keys.save(&mut api_key).await
    }
}
