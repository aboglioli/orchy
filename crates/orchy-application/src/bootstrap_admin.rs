use std::sync::Arc;

use orchy_core::error::Result;
use orchy_core::user::{Email, PlainPassword, User, UserId, UserStore};

use crate::dto::UserDto;

pub struct BootstrapAdmin {
    users: Arc<dyn UserStore>,
}

impl BootstrapAdmin {
    pub fn new(users: Arc<dyn UserStore>) -> Self {
        Self { users }
    }

    pub async fn execute(
        &self,
        hasher: &dyn orchy_core::user::PasswordHasher,
    ) -> Result<Option<UserDto>> {
        let existing_users = self.users.list_all().await?;
        if !existing_users.is_empty() {
            return Ok(None);
        }

        let email = Email::new("admin@orchy.sh")?;
        let password = PlainPassword::new("12345678")?;
        let id = UserId::new();

        let mut user = User::register_platform_admin(id, email, &password, hasher)?;
        self.users.save(&mut user).await?;

        Ok(Some(UserDto::from(&user)))
    }
}
