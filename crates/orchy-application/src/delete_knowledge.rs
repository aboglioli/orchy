use std::sync::Arc;

use orchy_core::edge::EdgeStore;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::parse_namespace;

pub struct DeleteKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
}

pub struct DeleteKnowledge {
    store: Arc<dyn KnowledgeStore>,
    edges: Arc<dyn EdgeStore>,
}

impl DeleteKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { store, edges }
    }

    pub async fn execute(&self, cmd: DeleteKnowledgeCommand) -> Result<()> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let path: KnowledgePath = cmd
            .path
            .parse::<KnowledgePath>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry: {path}")))?;

        let knowledge_id = entry.id().to_string();
        entry.mark_deleted()?;
        self.store.save(&mut entry).await?;
        self.store.delete(&entry.id()).await?;
        self.edges
            .delete_all_for(&org_id, &ResourceKind::Knowledge, &knowledge_id)
            .await
    }
}
