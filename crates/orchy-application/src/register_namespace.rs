use std::sync::Arc;

use orchy_core::error::{Error, Result};
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

    pub async fn execute(&self, cmd: RegisterNamespaceCommand) -> Result<()> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project = ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e))?;
        let namespace =
            Namespace::try_from(cmd.namespace).map_err(|e| Error::InvalidInput(e.to_string()))?;

        self.namespaces
            .register(&org_id, &project, &namespace)
            .await
    }
}
