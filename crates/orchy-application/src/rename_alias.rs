use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};

use crate::dto::AgentDto;

pub struct RenameAliasCommand {
    pub agent_id: String,
    pub new_alias: String,
}

pub struct RenameAlias {
    agents: Arc<dyn AgentStore>,
}

impl RenameAlias {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: RenameAliasCommand) -> Result<AgentDto> {
        let agent_id = AgentId::from_str(&cmd.agent_id)?;
        let new_alias =
            Alias::new(&cmd.new_alias).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        if agent.alias() != &new_alias {
            if let Some(existing) = self
                .agents
                .find_by_alias(agent.org_id(), agent.project(), &new_alias)
                .await?
                && existing.id() != &agent_id
            {
                return Err(Error::Conflict(format!(
                    "alias '{}' already taken",
                    cmd.new_alias
                )));
            }
            agent.set_alias(new_alias)?;
            self.agents.save(&mut agent).await?;
        }

        Ok(AgentDto::from(&agent))
    }
}
