use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    Email, OrgMembership, OrgMembershipStore, OrgRole, PlainPassword, User, UserId, UserStore,
};

use crate::dto::{OrgMembershipResponse, UserResponse};

fn map_org_error(e: orchy_events::Error) -> Error {
    Error::invalid_input(e.to_string())
}

pub struct InviteUserCommand {
    pub email: String,
    pub org_id: String,
    pub role: String,
    pub invited_by_user_id: String,
}

pub struct InviteUserResponse {
    pub user: UserResponse,
    pub membership: OrgMembershipResponse,
    pub is_new_user: bool,
}

pub struct InviteUser {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl InviteUser {
    pub fn new(users: Arc<dyn UserStore>, memberships: Arc<dyn OrgMembershipStore>) -> Self {
        Self { users, memberships }
    }

    pub async fn execute(
        &self,
        cmd: InviteUserCommand,
        hasher: &dyn orchy_core::user::PasswordHasher,
    ) -> Result<InviteUserResponse> {
        let org_id = OrganizationId::new(&cmd.org_id).map_err(map_org_error)?;
        let role = cmd.role.parse::<OrgRole>()?;

        let email = Email::new(&cmd.email)?;

        let (mut user, is_new_user) =
            if let Some(existing_user) = self.users.find_by_email(&email).await? {
                if !existing_user.is_active() {
                    return Err(Error::invalid_input("user is deactivated"));
                }
                (existing_user, false)
            } else {
                let temp_password = PlainPassword::new("changeme123")?;
                let id = UserId::new();
                let new_user = User::register(id, email, &temp_password, hasher)?;
                (new_user, true)
            };

        if self.memberships.find(user.id(), &org_id).await?.is_some() {
            return Err(Error::conflict(
                "user is already a member of this organization",
            ));
        }

        let membership = OrgMembership::new(*user.id(), org_id.clone(), role);
        
        // Record membership added event through user's event collector
        user.record_membership_added(
            &membership.id().to_string(),
            &org_id.to_string(),
            &role.to_string(),
        )?;
        
        self.memberships.save(&membership).await?;
        self.users.save(&mut user).await?;

        Ok(InviteUserResponse {
            user: UserResponse::from(&user),
            membership: OrgMembershipResponse::from(&membership),
            is_new_user,
        })
    }
}
