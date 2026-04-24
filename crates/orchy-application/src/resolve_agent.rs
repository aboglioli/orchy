use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::AgentDto;

pub struct ResolveAgentCommand {
    pub org_id: String,
    pub project: String,
    pub id_or_alias: String,
}

pub struct ResolveAgent {
    agents: Arc<dyn AgentStore>,
}

impl ResolveAgent {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: ResolveAgentCommand) -> Result<AgentDto> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project = ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e))?;

        if let Ok(agent_id) = cmd.id_or_alias.parse::<AgentId>()
            && let Some(agent) = self.agents.find_by_id(&agent_id).await?
            && agent.org_id() == &org
            && agent.project() == &project
        {
            return Ok(AgentDto::from(&agent));
        }

        let alias = Alias::new(&cmd.id_or_alias)?;
        let agent = self
            .agents
            .find_by_alias(&org, &project, &alias)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent '{}'", cmd.id_or_alias)))?;

        Ok(AgentDto::from(&agent))
    }
}
