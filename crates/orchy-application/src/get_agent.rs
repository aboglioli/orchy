use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::{Error, Result};

pub struct GetAgent {
    agents: Arc<dyn AgentStore>,
}

impl GetAgent {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, agent_id: &str) -> Result<Agent> {
        let id = AgentId::from_str(agent_id).map_err(Error::InvalidInput)?;
        self.agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }
}
