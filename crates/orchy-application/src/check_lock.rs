use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;

use crate::dto::ResourceLockDto;
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

    pub async fn execute(
        &self,
        cmd: CheckLockCommand,
    ) -> ApplicationResult<Option<ResourceLockDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
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
            Some(l) => Ok(Some(ResourceLockDto::from(&l))),
            None => Ok(None),
        }
    }
}
