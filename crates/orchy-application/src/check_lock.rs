use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::{LockStore, ResourceLock};

use crate::parse_namespace;

pub struct CheckLockCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub name: String,
}

pub struct CheckLock {
    store: Arc<dyn LockStore>,
}

impl CheckLock {
    pub fn new(store: Arc<dyn LockStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: CheckLockCommand) -> Result<Option<ResourceLock>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let lock = self
            .store
            .find(&org_id, &project, &namespace, &cmd.name)
            .await?;

        match lock {
            Some(l) if l.is_expired() => {
                self.store
                    .delete(&org_id, &project, &namespace, &cmd.name)
                    .await?;
                Ok(None)
            }
            other => Ok(other),
        }
    }
}
