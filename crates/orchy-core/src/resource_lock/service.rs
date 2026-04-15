use std::sync::Arc;

use super::{LockStore, ResourceLock};
use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;

pub struct LockService<S: LockStore> {
    store: Arc<S>,
}

impl<S: LockStore> LockService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn acquire(
        &self,
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        name: String,
        holder: AgentId,
        ttl_secs: u64,
    ) -> Result<ResourceLock> {
        if let Some(existing) = self.store.find(&org_id, &project, &namespace, &name).await? {
            if !existing.is_expired() && !existing.is_held_by(&holder) {
                return Err(Error::Conflict(format!(
                    "resource '{}' is locked by agent {}",
                    name,
                    existing.holder()
                )));
            }
            self.store.delete(&org_id, &project, &namespace, &name).await?;
        }

        let mut lock = ResourceLock::acquire(org_id, project, namespace, name, holder, ttl_secs)?;
        self.store.save(&mut lock).await?;
        Ok(lock)
    }

    pub async fn release(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
        holder: &AgentId,
    ) -> Result<()> {
        let mut lock = self
            .store
            .find(org, project, namespace, name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("lock '{name}'")))?;

        if !lock.is_held_by(holder) && !lock.is_expired() {
            return Err(Error::Conflict(format!(
                "lock '{}' is held by another agent",
                name
            )));
        }

        lock.mark_released();
        self.store.save(&mut lock).await?;
        self.store.delete(org, project, namespace, name).await
    }

    pub async fn release_agent_locks(&self, holder: &AgentId) -> Result<()> {
        let locks = self.store.find_by_holder(holder).await?;
        for mut lock in locks {
            lock.mark_released();
            self.store.save(&mut lock).await?;
            self.store
                .delete(lock.org_id(), lock.project(), lock.namespace(), lock.name())
                .await?;
        }
        Ok(())
    }

    pub async fn check(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        let lock = self.store.find(org, project, namespace, name).await?;
        match lock {
            Some(l) if l.is_expired() => {
                self.store.delete(org, project, namespace, name).await?;
                Ok(None)
            }
            other => Ok(other),
        }
    }
}
