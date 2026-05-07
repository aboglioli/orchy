use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::Error;
use orchy_core::user::{Email, PasswordHasher, PlainPassword, User, UserId, UserStore};

use crate::dto::UserDto;

pub struct RegisterUserCommand {
    pub email: String,
    pub password: String,
}

pub struct RegisterUserDto {
    pub user: UserDto,
}

pub struct RegisterUser {
    users: Arc<dyn UserStore>,
    hasher: Arc<dyn PasswordHasher>,
}

impl RegisterUser {
    pub fn new(users: Arc<dyn UserStore>, hasher: Arc<dyn PasswordHasher>) -> Self {
        Self { users, hasher }
    }

    pub async fn execute(&self, cmd: RegisterUserCommand) -> ApplicationResult<RegisterUserDto> {
        let email = Email::new(&cmd.email)?;
        let password = PlainPassword::new(&cmd.password)?;

        if self.users.find_by_email(&email).await?.is_some() {
            return Err(Error::conflict("user with this email already exists").into());
        }

        let id = UserId::new();
        let mut user = User::register(id, email, &password, self.hasher.as_ref())?;
        self.users.save(&mut user).await?;

        Ok(RegisterUserDto {
            user: UserDto::from(&user),
        })
    }
}
