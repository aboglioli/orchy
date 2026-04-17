use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::{Error, Result};

pub struct ChangeRolesCommand {
    pub agent_id: String,
    pub roles: Vec<String>,
}

pub struct ChangeRoles {
    agents: Arc<dyn AgentStore>,
}

impl ChangeRoles {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: ChangeRolesCommand) -> Result<Agent> {
        if cmd.roles.is_empty() {
            return Err(Error::InvalidInput("roles must not be empty".to_string()));
        }
        let id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.change_roles(cmd.roles)?;
        self.agents.save(&mut agent).await?;
        Ok(agent)
    }
}
