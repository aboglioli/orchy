use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;

use crate::parse_namespace;

pub struct UnlockResourceCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub name: String,
    pub holder_agent_id: String,
}

pub struct UnlockResource {
    store: Arc<dyn LockStore>,
}

impl UnlockResource {
    pub fn new(store: Arc<dyn LockStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: UnlockResourceCommand) -> Result<()> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let holder = AgentId::from_str(&cmd.holder_agent_id).map_err(Error::InvalidInput)?;

        let mut lock = self
            .store
            .find(&org_id, &project, &namespace, &cmd.name)
            .await?
            .ok_or_else(|| Error::NotFound(format!("lock '{}'", cmd.name)))?;

        if !lock.is_held_by(&holder) && !lock.is_expired() {
            return Err(Error::Conflict(format!(
                "lock '{}' is held by another agent",
                cmd.name
            )));
        }

        lock.mark_released()?;
        self.store.save(&mut lock).await?;
        self.store
            .delete(&org_id, &project, &namespace, &cmd.name)
            .await
    }
}
