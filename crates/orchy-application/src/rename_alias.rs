use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};

use crate::dto::AgentResponse;

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

    pub async fn execute(&self, cmd: RenameAliasCommand) -> Result<AgentResponse> {
        let agent_id = AgentId::from_str(&cmd.agent_id)?;
        let mut agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        let new_alias = Alias::new(&cmd.new_alias)?;

        if agent.alias().as_str() != cmd.new_alias {
            if let Some(existing) = self
                .agents
                .find_by_alias(agent.org_id(), agent.project(), &cmd.new_alias)
                .await?
            {
                if existing.id() != &agent_id {
                    return Err(Error::Conflict(format!(
                        "alias '{}' already taken",
                        cmd.new_alias
                    )));
                }
            }
            agent.set_alias(new_alias)?;
            self.agents.save(&mut agent).await?;
        }

        Ok(AgentResponse::from(&agent))
    }
}
