use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_core::user::{Email, User, UserId, UserStore};

use crate::MemoryState;

pub struct MemoryUserStore {
    state: Arc<MemoryState>,
}

impl MemoryUserStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl UserStore for MemoryUserStore {
    async fn save(&self, user: &mut User) -> Result<()> {
        let mut users = self.state.users.write().await;
        let mut by_email = self.state.user_by_email.write().await;

        let id = *user.id();
        let email = user.email().as_str().to_string();

        users.insert(id, user.clone());
        by_email.insert(email, id);

        user.drain_events();
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let users = self.state.users.read().await;
        Ok(users.get(id).cloned())
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let users = self.state.users.read().await;
        let by_email = self.state.user_by_email.read().await;

        Ok(by_email
            .get(email.as_str())
            .and_then(|id| users.get(id).cloned()))
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        let users = self.state.users.read().await;
        Ok(users.values().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPasswordHasher;

    impl orchy_core::user::PasswordHasher for MockPasswordHasher {
        fn hash(
            &self,
            plain: &orchy_core::user::PlainPassword,
        ) -> Result<orchy_core::user::HashedPassword> {
            orchy_core::user::HashedPassword::new(&format!("hashed_{}", plain.as_str()))
        }

        fn verify(
            &self,
            plain: &orchy_core::user::PlainPassword,
            hashed: &orchy_core::user::HashedPassword,
        ) -> Result<()> {
            let expected = format!("hashed_{}", plain.as_str());
            if hashed.as_str() == expected {
                Ok(())
            } else {
                Err(orchy_core::error::Error::authentication_failed(
                    "invalid password",
                ))
            }
        }
    }

    #[tokio::test]
    async fn user_crud() {
        let state = Arc::new(MemoryState::new());
        let store = MemoryUserStore::new(state);

        let email = Email::new("test@example.com").unwrap();
        let password = orchy_core::user::PlainPassword::new("password123").unwrap();

        let mut user =
            User::register(UserId::new(), email.clone(), &password, &MockPasswordHasher).unwrap();
        store.save(&mut user).await.unwrap();

        let found = store.find_by_email(&email).await.unwrap();
        assert!(found.is_some());

        let found = store.find_by_id(user.id()).await.unwrap();
        assert!(found.is_some());

        let all = store.list_all().await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
