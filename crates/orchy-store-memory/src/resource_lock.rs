use async_trait::async_trait;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock};

use crate::MemoryBackend;

#[async_trait]
impl LockStore for MemoryBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        {
            let mut locks = self
                .resource_locks
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            let key = (
                lock.org_id().to_string(),
                lock.project().to_string(),
                lock.namespace().to_string(),
                lock.name().to_string(),
            );
            locks.insert(key, lock.clone());
        }

        let events = lock.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
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
        let locks = self
            .resource_locks
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let key = (
            org.to_string(),
            project.to_string(),
            namespace.to_string(),
            name.to_string(),
        );
        Ok(locks.get(&key).cloned())
    }

    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        let mut locks = self
            .resource_locks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let key = (
            org.to_string(),
            project.to_string(),
            namespace.to_string(),
            name.to_string(),
        );
        locks.remove(&key);
        Ok(())
    }

    async fn find_by_holder(&self, holder: &AgentId) -> Result<Vec<ResourceLock>> {
        let locks = self
            .resource_locks
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(locks
            .values()
            .filter(|lock| *lock.holder() == *holder)
            .cloned()
            .collect())
    }

    async fn delete_expired(&self) -> Result<u64> {
        let mut locks = self
            .resource_locks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let before = locks.len();
        locks.retain(|_, lock| !lock.is_expired());
        Ok((before - locks.len()) as u64)
    }
}
