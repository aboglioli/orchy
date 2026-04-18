use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::{Organization, OrganizationStore};

pub struct ResolveApiKeyCommand<'a> {
    pub key: &'a str,
}

pub struct ResolveApiKey {
    orgs: Arc<dyn OrganizationStore>,
}

impl ResolveApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>) -> Self {
        Self { orgs }
    }

    pub async fn execute(&self, cmd: ResolveApiKeyCommand<'_>) -> Result<Option<Organization>> {
        self.orgs.find_by_api_key(cmd.key).await
    }
}
