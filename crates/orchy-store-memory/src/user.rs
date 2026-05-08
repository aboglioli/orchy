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
        let id = *user.id();
        let email = user.email().as_str().to_string();

        self.state.users.insert(id, user.clone());
        self.state.user_by_email.insert(email, id);

        user.drain_events();
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        Ok(self.state.users.get(id).map(|r| r.clone()))
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let id = match self.state.user_by_email.get(email.as_str()) {
            Some(r) => *r,
            None => return Ok(None),
        };
        Ok(self.state.users.get(&id).map(|r| r.clone()))
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        Ok(self.state.users.iter().map(|e| e.value().clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchy_core::error::DomainResult;
    use orchy_core::user::{HashedPassword, PasswordHasher, PlainPassword};

    struct MockPasswordHasher;

    impl PasswordHasher for MockPasswordHasher {
        fn hash(&self, plain: &PlainPassword) -> DomainResult<HashedPassword> {
            HashedPassword::new(&format!("hashed_{}", plain.as_str()))
        }

        fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> DomainResult<()> {
            let expected = format!("hashed_{}", plain.as_str());
            if hashed.as_str() == expected {
                Ok(())
            } else {
                Err(orchy_core::error::DomainError::PasswordMismatch)
            }
        }
    }

    #[tokio::test]
    async fn user_crud() {
        let state = Arc::new(MemoryState::new());
        let store = MemoryUserStore::new(state);

        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

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
