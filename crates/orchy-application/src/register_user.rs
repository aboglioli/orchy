use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::user::{Email, OrgMembershipStore, PlainPassword, User, UserId, UserStore};

use crate::dto::UserResponse;

pub struct RegisterUserCommand {
    pub email: String,
    pub password: String,
}

pub struct RegisterUserResponse {
    pub user: UserResponse,
}

pub struct RegisterUser {
    users: Arc<dyn UserStore>,
    memberships: Arc<dyn OrgMembershipStore>,
}

impl RegisterUser {
    pub fn new(users: Arc<dyn UserStore>, memberships: Arc<dyn OrgMembershipStore>) -> Self {
        Self { users, memberships }
    }

    pub async fn execute(
        &self,
        cmd: RegisterUserCommand,
        hasher: &dyn orchy_core::user::PasswordHasher,
    ) -> Result<RegisterUserResponse> {
        let email = Email::new(&cmd.email)?;
        let password = PlainPassword::new(&cmd.password)?;

        if self.users.find_by_email(&email).await?.is_some() {
            return Err(Error::conflict("user with this email already exists"));
        }

        let id = UserId::new();
        let mut user = User::register(id, email, &password, hasher)?;
        self.users.save(&mut user).await?;

        Ok(RegisterUserResponse {
            user: UserResponse::from(&user),
        })
    }
}
