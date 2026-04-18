use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{AgentResponse, PageResponse};

pub struct ListAgentsCommand {
    pub org_id: Option<String>,
    pub project: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct ListAgents {
    agents: Arc<dyn AgentStore>,
}

impl ListAgents {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: ListAgentsCommand) -> Result<PageResponse<AgentResponse>> {
        let org = cmd
            .org_id
            .map(|s| OrganizationId::new(&s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?
            .unwrap_or_else(|| OrganizationId::new("default").unwrap());

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.agents.list(&org, page).await?;

        let items: Vec<_> = if let Some(project) = project {
            result
                .items
                .iter()
                .filter(|a| *a.project() == project)
                .map(AgentResponse::from)
                .collect()
        } else {
            result.items.iter().map(AgentResponse::from).collect()
        };

        Ok(PageResponse {
            items,
            next_cursor: result.next_cursor,
        })
    }
}
