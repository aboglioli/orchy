use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{OrgMembershipStore, UserStore};

use crate::dto::UserResponse;

fn map_org_error(e: orchy_events::Error) -> Error {
    Error::invalid_input(e.to_string())
}

pub struct ListOrgUsersCommand {
    pub org_id: String,
}

pub struct ListOrgUsers {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl ListOrgUsers {
    pub fn new(users: Arc<dyn UserStore>, memberships: Arc<dyn OrgMembershipStore>) -> Self {
        Self { users, memberships }
    }

    pub async fn execute(&self, cmd: ListOrgUsersCommand) -> Result<Vec<UserResponse>> {
        let org_id = OrganizationId::new(&cmd.org_id).map_err(map_org_error)?;

        let memberships = self.memberships.find_by_org(&org_id).await?;
        let mut users = Vec::new();

        for membership in memberships {
            if let Some(user) = self.users.find_by_id(membership.user_id()).await? {
                users.push(UserResponse::from(&user));
            }
        }

        Ok(users)
    }
}
