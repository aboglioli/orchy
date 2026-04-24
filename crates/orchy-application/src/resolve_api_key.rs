use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;

use crate::dto::OrganizationDto;

#[derive(Debug, Clone)]
pub struct ApiKeyPrincipal {
    pub org: OrganizationDto,
    pub user_id: Option<String>,
}

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

    pub async fn execute(&self, cmd: ResolveApiKeyCommand) -> Result<Option<ApiKeyPrincipal>> {
        let org = self.orgs.find_by_api_key(&cmd.key).await?;
        let org = match org {
            Some(o) => o,
            None => return Ok(None),
        };

        let user_id = org
            .api_keys()
            .iter()
            .find(|k| k.is_active() && k.key() == cmd.key)
            .and_then(|k| k.user_id().map(|u| u.to_string()));

        Ok(Some(ApiKeyPrincipal {
            org: OrganizationDto::from(&org),
            user_id,
        }))
    }
}
