use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::user::{Email, PlainPassword, User, UserId, UserStore};

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
}

impl RegisterUser {
    pub fn new(users: Arc<dyn UserStore>) -> Self {
        Self { users }
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
