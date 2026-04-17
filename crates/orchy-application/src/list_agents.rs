use std::sync::Arc;

use orchy_core::agent::{Agent, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;

pub struct ListAgentsCommand {
    pub org_id: String,
}

pub struct ListAgents {
    agents: Arc<dyn AgentStore>,
}

impl ListAgents {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: ListAgentsCommand) -> Result<Vec<Agent>> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        self.agents.list(&org).await
    }
}
