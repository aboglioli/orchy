use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeFilter, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{KnowledgeResponse, PageResponse};
use crate::parse_namespace;

pub struct ListKnowledgeCommand {
    pub org_id: Option<String>,
    pub project: Option<String>,
    pub include_org_level: bool,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub path_prefix: Option<String>,
    pub agent_id: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
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
    ) -> Result<PageResponse<KnowledgeResponse>> {
        let org_id = cmd
            .org_id
            .map(|s| OrganizationId::new(&s))
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let project = cmd
            .project
            .map(ProjectId::try_from)
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let kind = cmd
            .kind
            .map(|s| s.parse::<KnowledgeKind>())
            .transpose()
            .map_err(Error::InvalidInput)?;

        let agent_id = cmd
            .agent_id
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let filter = KnowledgeFilter {
            org_id,
            project,
            include_org_level: cmd.include_org_level,
            namespace,
            kind,
            tag: cmd.tag,
            path_prefix: cmd.path_prefix,
            agent_id,
        };

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.store.list(filter, page).await?;
        Ok(PageResponse::from(result))
    }
}
