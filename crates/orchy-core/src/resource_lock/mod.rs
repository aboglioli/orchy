pub mod events;
pub mod service;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::future::Future;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

use self::events as lock_events;

pub trait LockStore: Send + Sync {
    fn save(&self, lock: &mut ResourceLock) -> impl Future<Output = Result<()>> + Send;
    fn find(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<Option<ResourceLock>>> + Send;
    fn delete(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<()>> + Send;
    fn delete_expired(&self) -> impl Future<Output = Result<u64>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLock {
    project: ProjectId,
    namespace: Namespace,
    name: String,
    holder: AgentId,
    acquired_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl ResourceLock {
    pub fn acquire(
        project: ProjectId,
        namespace: Namespace,
        name: String,
        holder: AgentId,
        ttl_secs: u64,
    ) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(Error::InvalidInput(
                "resource name must not be empty".into(),
            ));
        }

        let now = Utc::now();
        let mut lock = Self {
            project,
            namespace,
            name,
            holder,
            acquired_at: now,
            expires_at: now + Duration::seconds(ttl_secs as i64),
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            lock.project.as_ref(),
            lock_events::NAMESPACE,
            lock_events::TOPIC_ACQUIRED,
            Payload::from_json(&lock_events::LockAcquiredPayload {
                project: lock.project.to_string(),
                namespace: lock.namespace.to_string(),
                name: lock.name.clone(),
                holder: lock.holder.to_string(),
                ttl_secs,
            })
            .unwrap(),
        )
        .map(|e| lock.collector.collect(e));

        Ok(lock)
    }

    pub fn restore(r: RestoreResourceLock) -> Self {
        Self {
            project: r.project,
            namespace: r.namespace,
            name: r.name,
            holder: r.holder,
            acquired_at: r.acquired_at,
            expires_at: r.expires_at,
            collector: EventCollector::new(),
        }
    }

    pub fn mark_released(&mut self) {
        let _ = Event::create(
            self.project.as_ref(),
            lock_events::NAMESPACE,
            lock_events::TOPIC_RELEASED,
            Payload::from_json(&lock_events::LockReleasedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                name: self.name.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }

    pub fn is_held_by(&self, agent: &AgentId) -> bool {
        self.holder == *agent
    }

    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn holder(&self) -> AgentId {
        self.holder
    }
    pub fn acquired_at(&self) -> DateTime<Utc> {
        self.acquired_at
    }
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
}

pub struct RestoreResourceLock {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub name: String,
    pub holder: AgentId,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    #[test]
    fn acquire_succeeds() {
        let lock = ResourceLock::acquire(
            project(),
            Namespace::root(),
            "file.rs".into(),
            AgentId::new(),
            60,
        );
        assert!(lock.is_ok());
    }

    #[test]
    fn empty_name_fails() {
        let lock =
            ResourceLock::acquire(project(), Namespace::root(), "".into(), AgentId::new(), 60);
        assert!(lock.is_err());
    }

    #[test]
    fn not_expired_within_ttl() {
        let lock = ResourceLock::acquire(
            project(),
            Namespace::root(),
            "f".into(),
            AgentId::new(),
            3600,
        )
        .unwrap();
        assert!(!lock.is_expired());
    }

    #[test]
    fn is_held_by_holder() {
        let agent = AgentId::new();
        let lock =
            ResourceLock::acquire(project(), Namespace::root(), "f".into(), agent, 60).unwrap();
        assert!(lock.is_held_by(&agent));
        assert!(!lock.is_held_by(&AgentId::new()));
    }
}
