use std::str::FromStr;
use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationStore;
use orchy_core::user::{OrgMembershipStore, TokenEncoder, UserId};

use crate::dto::OrganizationDto;

#[derive(Debug, Clone)]
pub struct TokenPrincipal {
    pub org: OrganizationDto,
    pub user_id: String,
}

pub struct ResolveTokenCommand {
    pub token: String,
}

pub struct ResolveToken {
    token_encoder: Arc<dyn TokenEncoder>,
    memberships: Arc<dyn OrgMembershipStore>,
    orgs: Arc<dyn OrganizationStore>,
}

impl ResolveToken {
    pub fn new(
        token_encoder: Arc<dyn TokenEncoder>,
        memberships: Arc<dyn OrgMembershipStore>,
        orgs: Arc<dyn OrganizationStore>,
    ) -> Self {
        Self {
            token_encoder,
            memberships,
            orgs,
        }
    }

    pub async fn execute(&self, cmd: ResolveTokenCommand) -> Result<Option<TokenPrincipal>> {
        let claims = match self.token_encoder.decode(&cmd.token) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };

        let user_id = match UserId::from_str(&claims.sub) {
            Ok(id) => id,
            Err(_) => return Ok(None),
        };

        let memberships = self.memberships.find_by_user(&user_id).await?;
        let membership = match memberships.first() {
            Some(m) => m,
            None => return Ok(None),
        };

        let org = match self.orgs.find_by_id(membership.org_id()).await? {
            Some(o) => o,
            None => return Ok(None),
        };

        Ok(Some(TokenPrincipal {
            org: OrganizationDto::from(&org),
            user_id: claims.sub,
        }))
    }
}
