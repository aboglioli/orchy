use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

pub struct ListNamespacesCommand {
    pub org_id: String,
    pub project: String,
}

pub struct ListNamespaces {
    store: Arc<dyn NamespaceStore>,
}

impl ListNamespaces {
    pub fn new(store: Arc<dyn NamespaceStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: ListNamespacesCommand) -> Result<Vec<Namespace>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;

        self.store.list(&org_id, &project).await
    }
}
