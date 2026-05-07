use std::sync::Arc;

use crate::error::ApplicationResult;
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

    pub async fn execute(&self, cmd: ListNamespacesCommand) -> ApplicationResult<Vec<Namespace>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;

        self.store.list(&org_id, &project).await.map_err(Into::into)
    }
}
