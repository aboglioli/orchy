use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};

pub struct HeartbeatCommand {
    pub agent_id: String,
}

pub struct Heartbeat {
    agents: Arc<dyn AgentStore>,
}

impl Heartbeat {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: HeartbeatCommand) -> Result<()> {
        let id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.heartbeat()?;
        self.agents.save(&mut agent).await
    }
}
