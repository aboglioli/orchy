use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Resource};

use crate::dto::AgentDto;

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

    pub async fn execute(&self, cmd: ChangeRolesCommand) -> ApplicationResult<AgentDto> {
        if cmd.roles.is_empty() {
            return Err(Error::invalid_input("roles must not be empty".to_string()).into());
        }
        let id = AgentId::from_str(&cmd.agent_id)?;
        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Agent,
                id: id.to_string(),
            })?;
        agent.change_roles(cmd.roles)?;
        self.agents.save(&mut agent).await?;
        Ok(AgentDto::from(&agent))
    }
}
