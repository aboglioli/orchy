use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct TagKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub tag: String,
}

pub struct TagKnowledge {
    store: Arc<dyn KnowledgeStore>,
}

impl TagKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: TagKnowledgeCommand) -> Result<KnowledgeResponse> {
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

        entry.add_tag(cmd.tag)?;
        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
