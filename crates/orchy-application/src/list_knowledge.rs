use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::knowledge::{KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::Namespace;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{KnowledgeDto, PageResponse};

pub struct ListKnowledgeCommand {
    pub org_id: String,
    pub project: Option<String>,
    pub include_org_level: bool,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub orphaned: Option<bool>,
    pub archived: Option<bool>,
}

pub struct ListKnowledge {
    store: Arc<dyn KnowledgeStore>,
}

impl ListKnowledge {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(
        &self,
        cmd: ListKnowledgeCommand,
    ) -> ApplicationResult<PageResponse<KnowledgeDto>> {
        let org_id = Some(OrganizationId::new(&cmd.org_id)?);

        let project = cmd.project.map(ProjectId::try_from).transpose()?;

        let namespace = cmd
            .namespace
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(Namespace::new)
            .transpose()?;

        let kind = cmd.kind.map(|s| s.parse::<KnowledgeKind>()).transpose()?;

        let filter = KnowledgeFilter {
            org_id,
            project,
            include_org_level: cmd.include_org_level,
            namespace,
            kind,
            tag: cmd.tag,
            path_prefix: cmd.path_prefix,
            orphaned: cmd.orphaned,
            include_archived: cmd.archived,
            ..Default::default()
        };

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.store.list(filter, page).await?;
        Ok(PageResponse::from(result))
    }
}
