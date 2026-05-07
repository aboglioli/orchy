use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::user::{PasswordHasher, PlainPassword, UserId, UserStore};

use crate::dto::UserDto;

pub struct ChangePasswordCommand {
    pub user_id: String,
    pub old_password: String,
    pub new_password: String,
}

pub struct ChangePassword {
    users: Arc<dyn UserStore>,
    hasher: Arc<dyn PasswordHasher>,
}

impl ChangePassword {
    pub fn new(users: Arc<dyn UserStore>, hasher: Arc<dyn PasswordHasher>) -> Self {
        Self { users, hasher }
    }

    pub async fn execute(&self, cmd: ChangePasswordCommand) -> ApplicationResult<UserDto> {
        let user_id = UserId::from_str(&cmd.user_id)
            .map_err(|e| Error::invalid_input(format!("invalid user id: {}", e)))?;

        let old_password = PlainPassword::new(&cmd.old_password)?;
        let new_password = PlainPassword::new(&cmd.new_password)?;

        let mut user = self
            .users
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| Error::not_found(Resource::User, user_id.to_string()))?;

        user.change_password(&old_password, &new_password, self.hasher.as_ref())?;
        self.users.save(&mut user).await?;

        Ok(UserDto::from(&user))
    }
}
