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
    async fn acquire_if_free(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
        holder: &AgentId,
        ttl_secs: u64,
    ) -> Result<Option<ResourceLock>> {
        let key = lock_key(org, project, namespace, name);
        if let Some(existing) = self.state.resource_locks.get(&key) {
            if !existing.is_expired() && !existing.is_held_by(holder) {
                return Ok(None);
            }
        }
        let mut lock = ResourceLock::acquire(
            org.clone(),
            project.clone(),
            namespace.clone(),
            name.to_string(),
            holder.clone(),
            ttl_secs,
        )?;
        let events = lock.drain_events();
        self.state.resource_locks.insert(key, lock.clone());

        self.state.append_events(events).await?;
        Ok(Some(lock))
    }

    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        let key = lock_key(lock.org_id(), lock.project(), lock.namespace(), lock.name());
        self.state.resource_locks.insert(key, lock.clone());

        let events = lock.drain_events();
        self.state.append_events(events).await?;
        Ok(())
    }

    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let key = lock_key(org, project, namespace, name);
        Ok(self.state.resource_locks.get(&key).map(|r| r.clone()))
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let key = lock_key(org, project, namespace, name);
        self.state.resource_locks.remove(&key);
        Ok(())
    }

    async fn find_by_holder(
        &self,
        holder: &AgentId,
        org: &OrganizationId,
    ) -> Result<Vec<ResourceLock>> {
        Ok(self
            .state
            .resource_locks
            .iter()
            .filter(|entry| {
                let lock = entry.value();
                *lock.holder() == *holder && lock.org_id() == org
            })
            .map(|e| e.value().clone())
            .collect())
    }

    async fn release_for_agent(&self, holder: &AgentId, org: &OrganizationId) -> Result<u64> {
        let to_remove: Vec<String> = self
            .state
            .resource_locks
            .iter()
            .filter(|entry| {
                let lock = entry.value();
                lock.holder() == holder && lock.org_id() == org
            })
            .map(|e| e.key().clone())
            .collect();
        let count = to_remove.len() as u64;
        for key in to_remove {
            self.state.resource_locks.remove(&key);
        }
        Ok(count)
    }

    async fn delete_expired(&self) -> Result<u64> {
        let to_remove: Vec<String> = self
            .state
            .resource_locks
            .iter()
            .filter(|entry| entry.value().is_expired())
            .map(|e| e.key().clone())
            .collect();
        let count = to_remove.len() as u64;
        for key in to_remove {
            self.state.resource_locks.remove(&key);
        }
        Ok(count)
    }
}
