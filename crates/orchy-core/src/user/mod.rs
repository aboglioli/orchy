use chrono::{DateTime, Utc};
use orchy_events::{Event, EventCollector, Payload};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub mod email;
pub mod events;
pub mod id;
pub mod membership;
pub mod role;

pub use email::{Email, HashedPassword, PlainPassword};
pub use id::{MembershipId, UserId};
pub use membership::{OrgMembership, OrgMembershipStore, RestoreOrgMembership};
pub use role::OrgRole;

pub trait PasswordHasher: Send + Sync {
    fn hash(&self, plain: &PlainPassword) -> Result<HashedPassword>;
    fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenClaims {
    pub sub: String,
    pub email: String,
    pub iat: i64,
    pub exp: i64,
}

pub trait TokenEncoder: Send + Sync {
    fn encode(&self, user_id: &UserId, email: &Email) -> Result<String>;
    fn decode(&self, token: &str) -> Result<TokenClaims>;
}

#[async_trait::async_trait]
pub trait UserStore: Send + Sync {
    async fn save(&self, user: &mut User) -> Result<()>;
    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>>;
    async fn find_by_email(&self, email: &Email) -> Result<Option<User>>;
    async fn list_all(&self) -> Result<Vec<User>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    id: UserId,
    email: Email,
    password_hash: HashedPassword,
    is_active: bool,
    is_platform_admin: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

pub struct RestoreUser {
    pub id: UserId,
    pub email: Email,
    pub password_hash: HashedPassword,
    pub is_active: bool,
    pub is_platform_admin: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn register(
        id: UserId,
        email: Email,
        password: &PlainPassword,
        hasher: &dyn PasswordHasher,
    ) -> Result<Self> {
        let password_hash = hasher.hash(password)?;
        let now = Utc::now();

        let mut user = Self {
            id,
            email,
            password_hash,
            is_active: true,
            is_platform_admin: false,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&events::UserCreatedPayload {
            user_id: user.id.as_str(),
            email: user.email.as_str().to_string(),
            is_platform_admin: user.is_platform_admin,
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            user.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_CREATED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        user.collector.collect(event);

        Ok(user)
    }

    pub fn register_platform_admin(
        id: UserId,
        email: Email,
        password: &PlainPassword,
        hasher: &dyn PasswordHasher,
    ) -> Result<Self> {
        let password_hash = hasher.hash(password)?;
        let now = Utc::now();

        let mut user = Self {
            id,
            email,
            password_hash,
            is_active: true,
            is_platform_admin: true,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&events::UserCreatedPayload {
            user_id: user.id.as_str(),
            email: user.email.as_str().to_string(),
            is_platform_admin: user.is_platform_admin,
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            user.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_CREATED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        user.collector.collect(event);

        Ok(user)
    }

    pub fn restore(r: RestoreUser) -> Self {
        Self {
            id: r.id,
            email: r.email,
            password_hash: r.password_hash,
            is_active: r.is_active,
            is_platform_admin: r.is_platform_admin,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn login(&mut self, password: &PlainPassword, hasher: &dyn PasswordHasher) -> Result<()> {
        if !self.is_active {
            return Err(Error::authentication_failed("user is deactivated"));
        }

        match hasher.verify(password, &self.password_hash) {
            Ok(()) => {
                let payload = Payload::from_json(&events::UserLoginSucceededPayload {
                    user_id: self.id.as_str(),
                    email: self.email.as_str().to_string(),
                })
                .map_err(|e| Error::store(format!("event serialization: {e}")))?;

                let event = Event::create(
                    self.id.as_str(),
                    events::NAMESPACE,
                    events::TOPIC_LOGIN_SUCCEEDED,
                    payload,
                )
                .map_err(|e| Error::store(format!("event creation: {e}")))?;
                self.collector.collect(event);

                self.updated_at = Utc::now();
                Ok(())
            }
            Err(_) => {
                let payload = Payload::from_json(&events::UserLoginFailedPayload {
                    email: self.email.as_str().to_string(),
                    reason: "invalid password".to_string(),
                })
                .map_err(|e| Error::store(format!("event serialization: {e}")))?;

                let event = Event::create(
                    self.id.as_str(),
                    events::NAMESPACE,
                    events::TOPIC_LOGIN_FAILED,
                    payload,
                )
                .map_err(|e| Error::store(format!("event creation: {e}")))?;
                self.collector.collect(event);

                Err(Error::authentication_failed("invalid credentials"))
            }
        }
    }

    pub fn change_password(
        &mut self,
        old_password: &PlainPassword,
        new_password: &PlainPassword,
        hasher: &dyn PasswordHasher,
    ) -> Result<()> {
        hasher.verify(old_password, &self.password_hash)?;

        self.password_hash = hasher.hash(new_password)?;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&events::UserPasswordChangedPayload {
            user_id: self.id.as_str(),
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            self.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_PASSWORD_CHANGED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        Ok(())
    }

    pub fn deactivate(&mut self) -> Result<()> {
        self.is_active = false;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&events::UserDeactivatedPayload {
            user_id: self.id.as_str(),
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            self.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_DEACTIVATED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        Ok(())
    }

    pub fn make_platform_admin(&mut self) -> Result<()> {
        self.is_platform_admin = true;
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&events::UserPlatformAdminGrantedPayload {
            user_id: self.id.as_str(),
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            self.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_PLATFORM_ADMIN_GRANTED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        Ok(())
    }

    pub fn id(&self) -> &UserId {
        &self.id
    }

    pub fn email(&self) -> &Email {
        &self.email
    }

    pub fn password_hash(&self) -> &HashedPassword {
        &self.password_hash
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn is_platform_admin(&self) -> bool {
        self.is_platform_admin
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn record_membership_added(
        &mut self,
        membership_id: &str,
        org_id: &str,
        role: &str,
    ) -> Result<()> {
        let payload = Payload::from_json(&events::UserMembershipAddedPayload {
            membership_id: membership_id.to_string(),
            user_id: self.id.as_str(),
            org_id: org_id.to_string(),
            role: role.to_string(),
        })
        .map_err(|e| Error::store(format!("event serialization: {e}")))?;

        let event = Event::create(
            self.id.as_str(),
            events::NAMESPACE,
            events::TOPIC_MEMBERSHIP_ADDED,
            payload,
        )
        .map_err(|e| Error::store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockPasswordHasher;

    impl PasswordHasher for MockPasswordHasher {
        fn hash(&self, plain: &PlainPassword) -> Result<HashedPassword> {
            HashedPassword::new(&format!("hashed_{}", plain.as_str()))
        }

        fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> Result<()> {
            let expected = format!("hashed_{}", plain.as_str());
            if hashed.as_str() == expected {
                Ok(())
            } else {
                Err(Error::authentication_failed("invalid password"))
            }
        }
    }

    #[test]
    fn user_register() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();

        assert_eq!(user.email().as_str(), "test@example.com");
        assert!(user.is_active());
        assert!(!user.is_platform_admin());

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_CREATED);
    }

    #[test]
    fn user_login_success() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();
        user.drain_events();

        let result = user.login(&password, &hasher);
        assert!(result.is_ok());

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_LOGIN_SUCCEEDED);
    }

    #[test]
    fn user_login_wrong_password() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();
        user.drain_events();

        let wrong_password = PlainPassword::new("wrongpass").unwrap();
        let result = user.login(&wrong_password, &hasher);
        assert!(result.is_err());

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_LOGIN_FAILED);
    }

    #[test]
    fn user_login_deactivated() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();
        user.deactivate().unwrap();

        let result = user.login(&password, &hasher);
        assert!(result.is_err());
    }

    #[test]
    fn user_change_password() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let old_password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &old_password, &hasher).unwrap();
        user.drain_events();

        let new_password = PlainPassword::new("newpassword456").unwrap();
        let result = user.change_password(&old_password, &new_password, &hasher);
        assert!(result.is_ok());

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_PASSWORD_CHANGED);

        let old_login = user.login(&old_password, &hasher);
        assert!(old_login.is_err());

        let new_login = user.login(&new_password, &hasher);
        assert!(new_login.is_ok());
    }

    #[test]
    fn user_platform_admin() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("admin@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();
        let mut user = User::register_platform_admin(id, email, &password, &hasher).unwrap();

        assert!(user.is_platform_admin());

        // verify event was emitted
        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_CREATED);
    }

    #[test]
    fn user_record_membership_added() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();
        user.drain_events(); // Clear creation event

        // Record membership added
        user.record_membership_added("membership-123", "test-org", "member")
            .unwrap();

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].topic().as_str(), events::TOPIC_MEMBERSHIP_ADDED);
    }

    #[test]
    fn user_make_platform_admin_emits_event() {
        let hasher = MockPasswordHasher;
        let id = UserId::new();
        let email = Email::new("test@example.com").unwrap();
        let password = PlainPassword::new("password123").unwrap();

        let mut user = User::register(id, email, &password, &hasher).unwrap();
        user.drain_events(); // Clear creation event

        // Make platform admin
        user.make_platform_admin().unwrap();

        assert!(user.is_platform_admin());

        let events = user.drain_events();
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].topic().as_str(),
            events::TOPIC_PLATFORM_ADMIN_GRANTED
        );
    }
}
