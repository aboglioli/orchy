use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::agent::AgentId;
use orchy_core::error::Result;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock};

use crate::MemoryState;

fn lock_key(
    org: &OrganizationId,
    project: &ProjectId,
    namespace: &Namespace,
    name: &str,
) -> String {
    format!("{org}\0{project}\0{namespace}\0{name}")
}

pub struct MemoryLockStore {
    state: Arc<MemoryState>,
}

impl MemoryLockStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl LockStore for MemoryLockStore {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        {
            let mut locks = self.state.resource_locks.write().await;
            let key = lock_key(lock.org_id(), lock.project(), lock.namespace(), lock.name());
            locks.insert(key, lock.clone());
        }

        let events = lock.drain_events();
        if !events.is_empty() {
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| orchy_core::error::Error::Store(e.to_string()))?;
                self.state.events.write().await.push(serialized);
            }
        }
        Ok(())
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let locks = self.state.resource_locks.read().await;
        let key = lock_key(org, project, namespace, name);
        Ok(locks.get(&key).cloned())
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let mut locks = self.state.resource_locks.write().await;
        let key = lock_key(org, project, namespace, name);
        locks.remove(&key);
        Ok(())
    }

    async fn find_by_holder(
        &self,
        holder: &AgentId,
        org: &OrganizationId,
    ) -> Result<Vec<ResourceLock>> {
        let locks = self.state.resource_locks.read().await;
        Ok(locks
            .values()
            .filter(|lock| *lock.holder() == *holder && lock.org_id() == org)
            .cloned()
            .collect())
    }

    async fn delete_expired(&self) -> Result<u64> {
        let mut locks = self.state.resource_locks.write().await;
        let before = locks.len();
        locks.retain(|_, lock| !lock.is_expired());
        Ok((before - locks.len()) as u64)
    }
}
