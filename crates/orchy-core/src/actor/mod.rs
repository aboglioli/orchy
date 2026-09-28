mod identity;
mod lease;

use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

pub use identity::{ActorAlias, ActorId, MachineId};
pub use lease::{Lease, ResourceKey};

use crate::clock::Clock;
use crate::error::{DomainError, Result};
use crate::namespace::Namespace;

#[async_trait]
pub trait ActorStore: Send + Sync {
    async fn get(&self, id: &ActorId) -> Result<Option<Actor>>;
    async fn roster(&self) -> Result<Vec<Actor>>;
    async fn save(&self, actor: &mut Actor) -> Result<()>;
    async fn present(&self, now: DateTime<Utc>) -> Result<Vec<ActorId>>;
    async fn touch(&self, id: &ActorId, now: DateTime<Utc>) -> Result<()>;
}

#[async_trait]
pub trait LeaseStore: Send + Sync {
    async fn acquire(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease>;

    async fn renew(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease>;

    async fn release(&self, key: &ResourceKey, by: &ActorId) -> Result<()>;
    async fn check(&self, key: &ResourceKey) -> Result<Option<Lease>>;

    async fn held(&self) -> Result<Vec<Lease>>;
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Role(String);

impl Role {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.is_empty() {
            return Err(DomainError::validation("role must not be empty"));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::validation(format!(
                "role `{value}` may only contain lowercase letters, digits and `-`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Role {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Role {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Role> for String {
    fn from(role: Role) -> Self {
        role.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    id: ActorId,
    display_name: Option<String>,
    roles: Vec<Role>,
    namespace: Namespace,
    announced_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
}

impl Actor {
    pub fn new(
        id: ActorId,
        display_name: Option<String>,
        roles: Vec<Role>,
        namespace: Namespace,
        announced_at: DateTime<Utc>,
        last_seen: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            display_name,
            roles,
            namespace,
            announced_at,
            last_seen,
        }
    }

    pub fn announce(
        id: ActorId,
        roles: Vec<Role>,
        namespace: Namespace,
        clock: &dyn Clock,
    ) -> Self {
        let now = clock.now();
        Self::new(id, None, roles, namespace, now, now)
    }

    pub fn rename(&mut self, display_name: Option<String>) {
        self.display_name = display_name.filter(|n| !n.trim().is_empty());
    }

    pub fn set_roles(&mut self, roles: Vec<Role>) {
        self.roles = roles;
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        self.namespace = namespace;
    }

    pub fn seen_at(&mut self, now: DateTime<Utc>) {
        self.last_seen = now;
    }

    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }

    pub fn id(&self) -> &ActorId {
        &self.id
    }

    pub fn display_name(&self) -> Option<&str> {
        self.display_name.as_deref()
    }

    pub fn roles(&self) -> &[Role] {
        &self.roles
    }

    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }

    pub fn announced_at(&self) -> DateTime<Utc> {
        self.announced_at
    }

    pub fn last_seen(&self) -> DateTime<Utc> {
        self.last_seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn actor() -> Actor {
        Actor::announce(
            ActorId::new("claude", "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            vec![Role::new("reviewer").unwrap()],
            Namespace::root(),
            &FixedClock(at(1000)),
        )
    }

    #[test]
    fn announcing_records_the_roster_entry_and_the_first_sighting_together() {
        let actor = actor();
        assert_eq!(actor.announced_at(), at(1000));
        assert_eq!(actor.last_seen(), at(1000));
    }

    #[test]
    fn being_seen_does_not_rewrite_when_the_seat_was_created() {
        let mut actor = actor();
        actor.seen_at(at(5000));
        assert_eq!(actor.last_seen(), at(5000));
        assert_eq!(
            actor.announced_at(),
            at(1000),
            "a seat is not re-announced by activity"
        );
    }

    #[test]
    fn a_blank_display_name_is_treated_as_absent() {
        let mut actor = actor();
        actor.rename(Some("   ".to_owned()));
        assert_eq!(actor.display_name(), None);
        actor.rename(Some("Claude".to_owned()));
        assert_eq!(actor.display_name(), Some("Claude"));
    }

    #[test]
    fn roles_are_normalised_and_rejected_when_malformed() {
        assert_eq!(Role::new("Reviewer").unwrap().as_str(), "reviewer");
        assert!(Role::new("").is_err());
        assert!(Role::new("code reviewer").is_err());
        assert!(actor().has_role(&Role::new("REVIEWER").unwrap()));
    }
}
