use std::str::FromStr;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::user::{OrgMembershipStore, UserId, UserStore};

use crate::dto::{AuthResponse, OrgMembershipResponse, UserResponse};

pub struct GetCurrentUserCommand {
    pub user_id: String,
}

pub struct GetCurrentUser {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl GetCurrentUser {
    pub fn new(users: Arc<dyn UserStore>, memberships: Arc<dyn OrgMembershipStore>) -> Self {
        Self { users, memberships }
    }

    pub async fn execute(&self, cmd: GetCurrentUserCommand) -> Result<AuthResponse> {
        let user_id = UserId::from_str(&cmd.user_id)
            .map_err(|e| Error::invalid_input(format!("invalid user id: {}", e)))?;

        let user = self
            .users
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| Error::not_found("user"))?;

        if !user.is_active() {
            return Err(Error::authentication_failed("user is deactivated"));
        }

        let memberships = self.memberships.find_by_user(&user_id).await?;

        Ok(AuthResponse {
            user: UserResponse::from(&user),
            memberships: memberships
                .iter()
                .map(OrgMembershipResponse::from)
                .collect(),
        })
    }
}
