use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::identity::ActorId;
use crate::error::{DomainError, Result};
use crate::id::Id;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ResourceKey(String);

impl ResourceKey {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(DomainError::validation("resource key must not be empty"));
        }
        if value.contains(['\n', '\0']) {
            return Err(DomainError::validation(
                "resource key must not contain control characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn task(id: &Id) -> Self {
        Self(format!("task:{id}"))
    }

    pub fn document(id: &Id) -> Self {
        Self(format!("document:{id}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ResourceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ResourceKey {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for ResourceKey {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<ResourceKey> for String {
    fn from(key: ResourceKey) -> Self {
        key.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    resource: ResourceKey,
    holder: ActorId,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    generation: u64,
}

impl Lease {
    pub fn new(
        resource: ResourceKey,
        holder: ActorId,
        acquired_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        generation: u64,
    ) -> Self {
        Self {
            resource,
            holder,
            acquired_at,
            expires_at,
            generation,
        }
    }

    pub fn create(
        resource: ResourceKey,
        holder: ActorId,
        ttl: Duration,
        now: DateTime<Utc>,
        generation: u64,
    ) -> Self {
        Self::new(resource, holder, now, now + ttl, generation)
    }

    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    pub fn is_held_by(&self, actor: &ActorId, now: DateTime<Utc>) -> bool {
        !self.is_expired_at(now) && &self.holder == actor
    }

    pub fn renewed(&self, ttl: Duration, now: DateTime<Utc>) -> Self {
        Self {
            expires_at: now + ttl,
            ..self.clone()
        }
    }

    pub fn resource(&self) -> &ResourceKey {
        &self.resource
    }

    pub fn holder(&self) -> &ActorId {
        &self.holder
    }

    pub fn acquired_at(&self) -> DateTime<Utc> {
        self.acquired_at
    }

    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(alias: &str) -> ActorId {
        ActorId::new(alias, "01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap()
    }

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    fn lease() -> Lease {
        Lease::create(
            ResourceKey::new("build").unwrap(),
            actor("claude"),
            Duration::seconds(300),
            at(1000),
            1,
        )
    }

    #[test]
    fn expiry_is_evaluated_by_the_reader_not_a_daemon() {
        let lease = lease();
        assert!(!lease.is_expired_at(at(1299)));
        assert!(lease.is_expired_at(at(1300)), "expiry is inclusive");
        assert!(lease.is_expired_at(at(9999)));
    }

    #[test]
    fn an_expired_lease_is_held_by_nobody() {
        let lease = lease();
        assert!(lease.is_held_by(&actor("claude"), at(1100)));
        assert!(!lease.is_held_by(&actor("claude"), at(1300)));
        assert!(!lease.is_held_by(&actor("codex"), at(1100)));
    }

    #[test]
    fn renewing_extends_expiry_without_changing_holder_or_generation() {
        let lease = lease();
        let renewed = lease.renewed(Duration::seconds(300), at(1200));
        assert_eq!(renewed.expires_at(), at(1500));
        assert_eq!(renewed.holder(), lease.holder());
        assert_eq!(renewed.generation(), lease.generation());
        assert_eq!(renewed.acquired_at(), lease.acquired_at());
    }

    #[test]
    fn resource_keys_are_namespaced_by_entity() {
        let id = Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        assert_ne!(ResourceKey::task(&id), ResourceKey::document(&id));
        assert!(ResourceKey::task(&id).as_str().starts_with("task:"));
    }

    #[test]
    fn resource_key_rejects_empty_and_control_characters() {
        assert!(ResourceKey::new("").is_err());
        assert!(ResourceKey::new("  ").is_err());
        assert!(ResourceKey::new("a\nb").is_err());
    }
}
