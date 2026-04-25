use std::str::FromStr;
use std::sync::Arc;

use orchy_core::api_key::{ApiKeyId, ApiKeyStore};
use orchy_core::error::{Error, Result};

pub struct RevokeApiKeyCommand {
    pub key_id: String,
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
        self.api_keys.revoke(&key_id).await
    }
}
