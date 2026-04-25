pub mod events;

pub use orchy_events::OrganizationId;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use orchy_events::{Event, EventCollector, Payload};

use crate::error::{Error, Result};

use self::events as org_events;

#[async_trait::async_trait]
pub trait OrganizationStore: Send + Sync {
    async fn save(&self, org: &mut Organization) -> Result<()>;
    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>>;
    async fn list(&self) -> Result<Vec<Organization>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
    id: OrganizationId,
    name: String,
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
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
