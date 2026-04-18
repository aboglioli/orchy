use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;

use crate::dto::OrganizationResponse;

pub struct ResolveApiKeyCommand {
    pub key: String,
}

pub struct ResolveApiKey {
    orgs: Arc<dyn OrganizationStore>,
}

impl ResolveApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, cmd: ResolveApiKeyCommand) -> Result<Option<OrganizationResponse>> {
        let org = self.orgs.find_by_api_key(&cmd.key).await?;
        Ok(org.as_ref().map(OrganizationResponse::from))
    }
}
