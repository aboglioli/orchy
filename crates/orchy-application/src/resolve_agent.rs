use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

pub async fn resolve_agent(
    agents: &dyn AgentStore,
    org: &OrganizationId,
    project: &ProjectId,
    id_or_alias: &str,
) -> Result<Agent> {
    if let Ok(agent_id) = id_or_alias.parse::<AgentId>() {
        if let Some(agent) = agents.find_by_id(&agent_id).await? {
            return Ok(agent);
        }
    }
    agents
        .find_by_alias(org, project, id_or_alias)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent '{id_or_alias}'")))
}
