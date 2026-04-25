use std::sync::Arc;

use orchy_core::api_key::{ApiKeyGenerator, ApiKeyStore, PlainApiKey};
use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;

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
    api_keys: Arc<dyn ApiKeyStore>,
    orgs: Arc<dyn OrganizationStore>,
    generator: Arc<dyn ApiKeyGenerator>,
}

impl ResolveApiKey {
    pub fn new(
        api_keys: Arc<dyn ApiKeyStore>,
        orgs: Arc<dyn OrganizationStore>,
        generator: Arc<dyn ApiKeyGenerator>,
    ) -> Self {
        Self {
            api_keys,
            orgs,
            generator,
        }
    }

    pub async fn execute(&self, cmd: ResolveApiKeyCommand) -> Result<Option<ApiKeyPrincipal>> {
        let plain = match PlainApiKey::new(cmd.raw_key) {
            Ok(k) => k,
            Err(_) => return Ok(None),
        };

        let hashed = self.generator.hash(&plain);

        let api_key = match self.api_keys.find_by_hash(&hashed).await? {
            Some(k) if k.is_active() => k,
            _ => return Ok(None),
        };

        let org = match self.orgs.find_by_id(api_key.org_id()).await? {
            Some(o) => o,
            None => return Ok(None),
        };

        Ok(Some(ApiKeyPrincipal {
            org: OrganizationDto::from(&org),
            user_id: api_key.user_id().map(|u| u.to_string()),
        }))
    }
}
