use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::user::{Email, OrgMembershipStore, PlainPassword, UserStore};

use crate::dto::{AuthResponse, OrgMembershipResponse, UserResponse};

pub struct LoginUserCommand {
    pub email: String,
    pub password: String,
}

pub struct LoginUser {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl LoginUser {
    pub fn new(users: Arc<dyn UserStore>, memberships: Arc<dyn OrgMembershipStore>) -> Self {
        Self { users, memberships }
    }

    pub async fn execute(
        &self,
        cmd: LoginUserCommand,
        hasher: &dyn orchy_core::user::PasswordHasher,
    ) -> Result<AuthResponse> {
        let email = Email::new(&cmd.email)?;
        let password = PlainPassword::new(&cmd.password)?;

        let mut user = self
            .users
            .find_by_email(&email)
            .await?
            .ok_or_else(|| Error::authentication_failed("invalid credentials"))?;

        user.login(&password, hasher)?;
        self.users.save(&mut user).await?;

        let memberships = self.memberships.find_by_user(user.id()).await?;

        Ok(AuthResponse {
            user: UserResponse::from(&user),
            memberships: memberships
                .iter()
                .map(OrgMembershipResponse::from)
                .collect(),
        })
    }
}
