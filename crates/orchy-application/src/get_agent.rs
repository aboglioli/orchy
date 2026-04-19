use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};

use crate::dto::AgentResponse;

pub struct GetAgentCommand {
    pub agent_id: String,
}

pub struct GetAgent {
    agents: Arc<dyn AgentStore>,
}

impl GetAgent {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: GetAgentCommand) -> Result<AgentResponse> {
        let id = AgentId::from_str(&cmd.agent_id)?;
        let agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        Ok(AgentResponse::from(agent))
    }
}
