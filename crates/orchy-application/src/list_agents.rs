use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{AgentResponse, PageResponse};

pub struct ListAgentsCommand {
    pub org_id: String,
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
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        if let Some(ref project) = project {
            let limit = cmd.limit.unwrap_or(50) as usize;
            let result = self.agents.list(&org, PageParams::unbounded()).await?;

            let filtered: Vec<AgentResponse> = result
                .items
                .iter()
                .filter(|a| a.project() == project)
                .map(AgentResponse::from)
                .collect();

            let start = if let Some(ref cursor) = cmd.after {
                filtered
                    .iter()
                    .position(|a| a.id == *cursor)
                    .map(|i| i + 1)
                    .unwrap_or(0)
            } else {
                0
            };

            let page_items: Vec<AgentResponse> =
                filtered.into_iter().skip(start).take(limit + 1).collect();
            let has_more = page_items.len() > limit;
            let items: Vec<AgentResponse> = page_items.into_iter().take(limit).collect();
            let next_cursor = if has_more {
                items.last().map(|a| a.id.clone())
            } else {
                None
            };

            return Ok(PageResponse { items, next_cursor });
        }

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.agents.list(&org, page).await?;

        Ok(PageResponse {
            items: result.items.iter().map(AgentResponse::from).collect(),
            next_cursor: result.next_cursor,
        })
    }
}
