use std::sync::Arc;

use orchy_core::api_key::ApiKeyGenerator;
use orchy_core::error::Result;
use orchy_core::organization::{OrganizationStore, RawApiKey};

use crate::dto::OrganizationDto;

#[derive(Debug, Clone)]
pub struct ApiKeyPrincipal {
    pub org: OrganizationDto,
    pub user_id: Option<String>,
}

pub struct ResolveApiKeyCommand {
    pub raw_key: String,
}

pub struct ResolveApiKey {
    orgs: Arc<dyn OrganizationStore>,
    generator: Arc<dyn ApiKeyGenerator>,
}

impl ResolveApiKey {
    pub fn new(orgs: Arc<dyn OrganizationStore>, generator: Arc<dyn ApiKeyGenerator>) -> Self {
        Self { orgs, generator }
    }

    pub async fn execute(&self, cmd: ResolveApiKeyCommand) -> Result<Option<ApiKeyPrincipal>> {
        let raw_key = match RawApiKey::new(cmd.raw_key) {
            Ok(k) => k,
            Err(_) => return Ok(None),
        };
        let key_hash = self.generator.hash(&raw_key)?;

        let org = self.orgs.find_by_api_key_hash(key_hash.as_str()).await?;
        let org = match org {
            Some(o) => o,
            None => return Ok(None),
        };

        let user_id = org
            .api_keys()
            .iter()
            .find(|k| k.is_active() && k.key_hash() == &key_hash)
            .and_then(|k| k.user_id().map(|u| u.to_string()));

        Ok(Some(ApiKeyPrincipal {
            org: OrganizationDto::from(&org),
            user_id,
        }))
    }
}
