use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::user::{Email, PasswordHasher, PlainPassword, User, UserId, UserStore};

use crate::dto::UserDto;

pub struct BootstrapAdmin {
    users: Arc<dyn UserStore>,
    hasher: Arc<dyn PasswordHasher>,
}

impl BootstrapAdmin {
    pub fn new(users: Arc<dyn UserStore>, hasher: Arc<dyn PasswordHasher>) -> Self {
        Self { users, hasher }
    }

    pub async fn execute(&self) -> Result<Option<UserDto>> {
        let existing_users = self.users.list_all().await?;
        if !existing_users.is_empty() {
            return Ok(None);
        }

        let email = Email::new("admin@orchy.sh")?;
        let password = PlainPassword::new("12345678")?;
        let id = UserId::new();

        let mut user = User::register_platform_admin(id, email, &password, self.hasher.as_ref())?;
        self.users.save(&mut user).await?;

        Ok(Some(UserDto::from(&user)))
    }
}
