use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct ReadKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
}

pub struct ReadKnowledge {
    store: Arc<dyn KnowledgeStore>,
}

impl ReadKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: ReadKnowledgeCommand) -> Result<Option<Knowledge>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        self.store
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.path)
            .await
    }
}
