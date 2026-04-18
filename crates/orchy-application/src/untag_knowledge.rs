use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct UntagKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub tag: String,
}

pub struct UntagKnowledge {
    store: Arc<dyn KnowledgeStore>,
}

impl UntagKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: UntagKnowledgeCommand) -> Result<KnowledgeResponse> {
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

        entry.remove_tag(&cmd.tag)?;
        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
