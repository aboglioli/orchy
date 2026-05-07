use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::knowledge::{KnowledgeKind, KnowledgeStore};

use crate::dto::KnowledgeDto;
use crate::parse_namespace;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::list_skills::ListSkills;

pub struct ListOverviewsCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct ListOverviews {
    inner: ListSkills,
}

impl ListOverviews {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self {
            inner: ListSkills::new(store),
        }
    }

    pub async fn execute(&self, cmd: ListOverviewsCommand) -> ApplicationResult<Vec<KnowledgeDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let entries = self
            .inner
            .list_with_inheritance(&org_id, &project, &namespace, KnowledgeKind::Overview)
            .await?;
        Ok(entries.iter().map(KnowledgeDto::from).collect())
    }
}
