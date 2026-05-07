use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::OrganizationId;

pub struct RegisterNamespaceCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: String,
}

pub struct RegisterNamespace {
    namespaces: Arc<dyn NamespaceStore>,
}

impl RegisterNamespace {
    pub fn new(namespaces: Arc<dyn NamespaceStore>) -> Self {
        Self { namespaces }
    }

    pub async fn execute(&self, cmd: RegisterNamespaceCommand) -> ApplicationResult<()> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = Namespace::try_from(cmd.namespace)?;

        self.namespaces
            .register(&org_id, &project, &namespace)
            .await
            .map_err(Into::into)
    }
}
