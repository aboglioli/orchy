use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Resource};

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

    pub async fn execute(&self, cmd: HeartbeatCommand) -> ApplicationResult<()> {
        let id = AgentId::from_str(&cmd.agent_id)?;
        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Agent,
                id: id.to_string(),
            })?;
        agent.heartbeat()?;
        self.agents.save(&mut agent).await.map_err(Into::into)
    }
}
