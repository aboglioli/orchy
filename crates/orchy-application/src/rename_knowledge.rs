use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct RenameKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub new_path: String,
}

pub struct RenameKnowledge {
    store: Arc<dyn KnowledgeStore>,
}

impl RenameKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: RenameKnowledgeCommand) -> Result<Knowledge> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry: {}", cmd.path)))?;

        entry.rename(cmd.new_path)?;
        self.store.save(&mut entry).await?;
        Ok(entry)
    }
}
