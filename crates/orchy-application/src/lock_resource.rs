use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock};

use crate::parse_namespace;

pub struct LockResourceCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub name: String,
    pub holder_agent_id: String,
    pub ttl_secs: Option<u64>,
}

pub struct LockResource {
    store: Arc<dyn LockStore>,
}

impl LockResource {
    pub fn new(store: Arc<dyn LockStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: LockResourceCommand) -> Result<ResourceLock> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let holder = AgentId::from_str(&cmd.holder_agent_id).map_err(Error::InvalidInput)?;
        let ttl_secs = cmd.ttl_secs.unwrap_or(300);

        if let Some(existing) = self
            .store
            .find(&org_id, &project, &namespace, &cmd.name)
            .await?
        {
            if !existing.is_expired() && !existing.is_held_by(&holder) {
                return Err(Error::Conflict(format!(
                    "resource '{}' is locked by agent {}",
                    cmd.name,
                    existing.holder()
                )));
            }
            self.store
                .delete(&org_id, &project, &namespace, &cmd.name)
                .await?;
        }

        let mut lock =
            ResourceLock::acquire(org_id, project, namespace, cmd.name, holder, ttl_secs)?;
        self.store.save(&mut lock).await?;
        Ok(lock)
    }
}
