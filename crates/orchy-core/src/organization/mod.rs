pub mod events;

pub use orchy_events::OrganizationId;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};
use crate::user::UserId;

use self::events as org_events;

#[async_trait::async_trait]
pub trait OrganizationStore: Send + Sync {
    async fn save(&self, org: &mut Organization) -> Result<()>;
    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>>;
    async fn find_by_api_key(&self, key: &str) -> Result<Option<Organization>>;
    async fn list(&self) -> Result<Vec<Organization>>;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    id: ApiKeyId,
    name: String,
    key: String,
    user_id: Option<UserId>,
    is_active: bool,
    created_at: DateTime<Utc>,
}

impl ApiKey {
    fn new(name: String, key: String, user_id: Option<UserId>) -> Self {
        Self {
            id: ApiKeyId::new(),
            name,
            key,
            user_id,
            is_active: true,
            created_at: Utc::now(),
        }
    }

    pub fn id(&self) -> &ApiKeyId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key(&self) -> &str {
        &self.key
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    id: OrganizationId,
    name: String,
    api_keys: Vec<ApiKey>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Organization {
    pub fn new(id: OrganizationId, name: String) -> Result<Self> {
        let now = Utc::now();
        let mut org = Self {
            id,
            name,
            api_keys: vec![],
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        let payload = Payload::from_json(&org_events::OrgCreatedPayload {
            org_id: org.id.to_string(),
            name: org.name.clone(),
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            org.id.as_str(),
            org_events::NAMESPACE,
            org_events::TOPIC_CREATED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        org.collector.collect(event);

        Ok(org)
    }

    pub fn restore(r: RestoreOrganization) -> Self {
        Self {
            id: r.id,
            name: r.name,
            api_keys: r.api_keys,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn add_api_key(
        &mut self,
        name: String,
        key: String,
        user_id: Option<UserId>,
    ) -> Result<&ApiKey> {
        let api_key = ApiKey::new(name.clone(), key.clone(), user_id);
        let key_id = api_key.id().to_string();
        self.api_keys.push(api_key);
        self.updated_at = Utc::now();

        let payload = Payload::from_json(&org_events::ApiKeyAddedPayload {
            org_id: self.id.to_string(),
            key_id,
            name,
        })
        .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
        let event = Event::create(
            self.id.as_str(),
            org_events::NAMESPACE,
            org_events::TOPIC_API_KEY_ADDED,
            payload,
        )
        .map_err(|e| Error::Store(format!("event creation: {e}")))?;
        self.collector.collect(event);

        self.api_keys
            .last()
            .ok_or_else(|| Error::Store("api_keys empty after push".into()))
    }

    pub fn revoke_api_key(&mut self, key_id: &ApiKeyId) -> Result<()> {
        if let Some(k) = self.api_keys.iter_mut().find(|k| k.id() == key_id) {
            k.is_active = false;
            self.updated_at = Utc::now();

            let payload = Payload::from_json(&org_events::ApiKeyRevokedPayload {
                org_id: self.id.to_string(),
                key_id: key_id.to_string(),
            })
            .map_err(|e| Error::Store(format!("event serialization: {e}")))?;
            let event = Event::create(
                self.id.as_str(),
                org_events::NAMESPACE,
                org_events::TOPIC_API_KEY_REVOKED,
                payload,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?;
            self.collector.collect(event);
        }
        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<orchy_events::Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> &OrganizationId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn api_keys(&self) -> &[ApiKey] {
        &self.api_keys
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

pub struct RestoreOrganization {
    pub id: OrganizationId,
    pub name: String,
    pub api_keys: Vec<ApiKey>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
