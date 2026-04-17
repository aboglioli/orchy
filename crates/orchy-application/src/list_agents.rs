use std::sync::Arc;

use orchy_core::agent::{Agent, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

pub struct ListAgentsCommand {
    pub org_id: String,
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

    pub async fn execute(&self, cmd: ListAgentsCommand) -> Result<Page<Agent>> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let page = PageParams::new(cmd.after, cmd.limit);
        self.agents.list(&org, page).await
    }
}
