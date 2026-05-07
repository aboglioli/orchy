use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::KnowledgeDto;
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

    pub async fn execute(&self, cmd: RenameKnowledgeCommand) -> ApplicationResult<KnowledgeDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let path: KnowledgePath = cmd.path.parse::<KnowledgePath>()?;
        let new_path: KnowledgePath = cmd.new_path.parse::<KnowledgePath>()?;

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &path)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Knowledge,
                id: path.to_string(),
            })?;

        entry.rename(new_path)?;
        self.store.save(&mut entry).await?;
        Ok(KnowledgeDto::from(&entry))
    }
}
