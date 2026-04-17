use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStatus, AgentStore};
use orchy_core::error::{Error, Result};

pub struct UpdateAgentStatusCommand {
    pub agent_id: String,
    pub status: AgentStatus,
}

pub struct UpdateAgentStatus {
    agents: Arc<dyn AgentStore>,
}

impl UpdateAgentStatus {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: UpdateAgentStatusCommand) -> Result<()> {
        let id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.update_status(cmd.status)?;
        self.agents.save(&mut agent).await
    }
}
