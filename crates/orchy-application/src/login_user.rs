use std::sync::Arc;

use serde::Serialize;

use orchy_core::error::{Error, Result};
use orchy_core::user::{
    Email, OrgMembershipStore, PasswordHasher, PlainPassword, TokenEncoder, UserStore,
};

use crate::dto::{OrgMembershipDto, UserDto};

pub struct LoginUserCommand {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoginUserResponse {
    pub user: UserDto,
    pub memberships: Vec<OrgMembershipDto>,
    pub token: String,
}

pub struct LoginUser {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
    token_encoder: Arc<dyn TokenEncoder>,
    hasher: Arc<dyn PasswordHasher>,
}

impl LoginUser {
    pub fn new(
        users: Arc<dyn UserStore>,
        memberships: Arc<dyn OrgMembershipStore>,
        token_encoder: Arc<dyn TokenEncoder>,
        hasher: Arc<dyn PasswordHasher>,
    ) -> Self {
        Self {
            users,
            memberships,
            token_encoder,
            hasher,
        }
    }

    pub async fn execute(&self, cmd: LoginUserCommand) -> Result<LoginUserResponse> {
        let email = Email::new(&cmd.email)?;
        let password = PlainPassword::new(&cmd.password)?;

        let mut user = self
            .users
            .find_by_email(&email)
            .await?
            .ok_or_else(|| Error::authentication_failed("invalid credentials"))?;

        user.login(&password, self.hasher.as_ref())?;
        self.users.save(&mut user).await?;

        let token = self.token_encoder.encode(user.id(), user.email())?;
        let memberships = self.memberships.find_by_user(user.id()).await?;

        Ok(LoginUserResponse {
            user: UserDto::from(&user),
            memberships: memberships.iter().map(OrgMembershipDto::from).collect(),
            token,
        })
    }
}
