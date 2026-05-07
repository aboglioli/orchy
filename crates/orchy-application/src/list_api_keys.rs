use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::api_key::ApiKeyStore;
use orchy_core::organization::OrganizationId;

use crate::dto::ApiKeyDto;

pub struct ListApiKeysCommand {
    pub org_id: String,
}

pub struct ListApiKeys {
    api_keys: Arc<dyn ApiKeyStore>,
}

impl ListApiKeys {
    pub fn new(api_keys: Arc<dyn ApiKeyStore>) -> Self {
        Self { api_keys }
    }

    pub async fn execute(&self, cmd: ListApiKeysCommand) -> ApplicationResult<Vec<ApiKeyDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let keys = self.api_keys.find_by_org(&org_id).await?;
        Ok(keys.iter().map(ApiKeyDto::from).collect())
    }
}
