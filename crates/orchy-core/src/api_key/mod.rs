pub mod events;

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use orchy_events::{Event, EventCollector, Payload};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::organization::OrganizationId;
use crate::user::UserId;

pub trait ApiKeyGenerator: Send + Sync {
    fn generate(
        &self,
        org_id: &OrganizationId,
        user_id: Option<UserId>,
        name: String,
    ) -> Result<(PlainApiKey, ApiKey)>;

    fn hash(&self, plain: &PlainApiKey) -> HashedApiKey;
}

#[async_trait::async_trait]
pub trait ApiKeyStore: Send + Sync {
    async fn save(&self, api_key: &mut ApiKey) -> Result<()>;
    async fn find_by_id(&self, id: &ApiKeyId) -> Result<Option<ApiKey>>;
    async fn find_by_hash(&self, hash: &HashedApiKey) -> Result<Option<ApiKey>>;
    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<ApiKey>>;
}

const KEY_PREFIX: &str = "sk_";
const KEY_LEN: usize = 67;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlainApiKey(String);

impl PlainApiKey {
    pub fn new(s: String) -> Result<Self> {
        if !s.starts_with(KEY_PREFIX) || s.len() != KEY_LEN {
            return Err(Error::invalid_input(
                "API key must be 'sk_' followed by 64 hex characters",
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn prefix(&self) -> &str {
        &self.0[..8]
    }

    pub fn suffix(&self) -> &str {
        &self.0[self.0.len() - 4..]
    }
}

impl fmt::Display for PlainApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashedApiKey(String);

impl HashedApiKey {
    pub fn new(s: String) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::invalid_input("API key hash cannot be empty"));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for HashedApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKeyPrefix(String);

impl ApiKeyPrefix {
    pub fn new(s: String) -> Result<Self> {
        if s.len() != 8 {
            return Err(Error::invalid_input(
                "API key prefix must be exactly 8 characters",
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApiKeyPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKeySuffix(String);

impl ApiKeySuffix {
    pub fn new(s: String) -> Result<Self> {
        if s.len() != 4 {
            return Err(Error::invalid_input(
                "API key suffix must be exactly 4 characters",
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApiKeySuffix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKeyId(Uuid);

impl ApiKeyId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ApiKeyId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ApiKeyId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ApiKeyId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|e| Error::invalid_input(format!("invalid api key id: {e}")))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    id: ApiKeyId,
    org_id: OrganizationId,
    name: String,
    hashed_key: HashedApiKey,
    key_prefix: ApiKeyPrefix,
    key_suffix: ApiKeySuffix,
    user_id: Option<UserId>,
    is_active: bool,
    created_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

pub struct RestoreApiKey {
    pub id: ApiKeyId,
    pub org_id: OrganizationId,
    pub name: String,
    pub hashed_key: HashedApiKey,
    pub key_prefix: ApiKeyPrefix,
    pub key_suffix: ApiKeySuffix,
    pub user_id: Option<UserId>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn new(
        org_id: OrganizationId,
        name: String,
        hashed_key: HashedApiKey,
        key_prefix: ApiKeyPrefix,
        key_suffix: ApiKeySuffix,
        user_id: Option<UserId>,
    ) -> Self {
        let mut key = Self {
            id: ApiKeyId::new(),
            org_id,
            name,
            hashed_key,
            key_prefix,
            key_suffix,
            user_id,
            is_active: true,
            created_at: Utc::now(),
            collector: EventCollector::new(),
        };
        let payload = Payload::from_json(&events::ApiKeyCreatedPayload {
            org_id: key.org_id.to_string(),
            api_key_id: key.id.to_string(),
            name: key.name.clone(),
            user_id: key.user_id.as_ref().map(|u| u.to_string()),
        })
        .expect("api_key.created payload always serializes");
        let event = Event::create(
            key.org_id.as_str(),
            events::NAMESPACE,
            events::TOPIC_CREATED,
            key.id.to_string(),
            payload,
        )
        .expect("api_key.created event always constructs");
        key.collector.collect(event);
        key
    }

    pub fn restore(r: RestoreApiKey) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            name: r.name,
            hashed_key: r.hashed_key,
            key_prefix: r.key_prefix,
            key_suffix: r.key_suffix,
            user_id: r.user_id,
            is_active: r.is_active,
            created_at: r.created_at,
            collector: EventCollector::new(),
        }
    }

    pub fn revoke(&mut self) {
        if !self.is_active {
            return;
        }
        self.is_active = false;
        let payload = Payload::from_json(&events::ApiKeyRevokedPayload {
            org_id: self.org_id.to_string(),
            api_key_id: self.id.to_string(),
        })
        .expect("api_key.revoked payload always serializes");
        let event = Event::create(
            self.org_id.as_str(),
            events::NAMESPACE,
            events::TOPIC_REVOKED,
            self.id.to_string(),
            payload,
        )
        .expect("api_key.revoked event always constructs");
        self.collector.collect(event);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> &ApiKeyId {
        &self.id
    }

    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn hashed_key(&self) -> &HashedApiKey {
        &self.hashed_key
    }

    pub fn key_prefix(&self) -> &ApiKeyPrefix {
        &self.key_prefix
    }

    pub fn key_suffix(&self) -> &ApiKeySuffix {
        &self.key_suffix
    }

    pub fn user_id(&self) -> Option<&UserId> {
        self.user_id.as_ref()
    }

    pub fn is_active(&self) -> bool {
        self.is_active
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_api_key_valid() {
        let key = PlainApiKey::new(
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        assert_eq!(key.prefix(), "sk_abcde");
        assert_eq!(key.suffix(), "7890");
    }

    #[test]
    fn plain_api_key_rejects_short() {
        assert!(PlainApiKey::new("sk_short".to_string()).is_err());
    }

    #[test]
    fn plain_api_key_rejects_bad_prefix() {
        assert!(
            PlainApiKey::new(
                "pk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn hashed_api_key_rejects_empty() {
        assert!(HashedApiKey::new("".to_string()).is_err());
    }

    #[test]
    fn api_key_prefix_rejects_wrong_length() {
        assert!(ApiKeyPrefix::new("short".to_string()).is_err());
    }

    #[test]
    fn api_key_suffix_rejects_wrong_length() {
        assert!(ApiKeySuffix::new("ab".to_string()).is_err());
    }

    fn make_key() -> ApiKey {
        let org_id = OrganizationId::new("test-org").unwrap();
        let hash = HashedApiKey::new("somehash".to_string()).unwrap();
        let prefix = ApiKeyPrefix::new("sk_abcde".to_string()).unwrap();
        let suffix = ApiKeySuffix::new("7890".to_string()).unwrap();
        ApiKey::new(org_id, "Production".into(), hash, prefix, suffix, None)
    }

    #[test]
    fn api_key_new_and_revoke() {
        let org_id = OrganizationId::new("test-org").unwrap();
        let mut key = make_key();
        assert_eq!(key.org_id(), &org_id);
        assert_eq!(key.name(), "Production");
        assert!(key.is_active());
        assert!(key.user_id().is_none());

        key.revoke();
        assert!(!key.is_active());
    }

    #[test]
    fn new_collects_one_created_event() {
        let mut key = make_key();
        let evts = key.drain_events();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].topic().as_str(), events::TOPIC_CREATED);
    }

    #[test]
    fn revoke_collects_one_revoked_event() {
        let mut key = make_key();
        key.drain_events();
        key.revoke();
        let evts = key.drain_events();
        assert_eq!(evts.len(), 1);
        assert_eq!(evts[0].topic().as_str(), events::TOPIC_REVOKED);
    }

    #[test]
    fn revoke_idempotent_when_inactive() {
        let mut key = make_key();
        key.drain_events();
        key.revoke();
        key.drain_events();
        key.revoke();
        let evts = key.drain_events();
        assert!(evts.is_empty());
    }
}
