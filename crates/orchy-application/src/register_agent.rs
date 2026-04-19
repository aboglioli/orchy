use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, RegisterAgent as DomainRegisterAgent};
use orchy_core::error::{Error, Result};

use crate::dto::AgentResponse;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct RegisterAgentCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub roles: Vec<String>,
    pub description: String,
    pub id: Option<String>,
    pub parent_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

pub struct RegisterAgent {
    agents: Arc<dyn AgentStore>,
}

impl RegisterAgent {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, cmd: RegisterAgentCommand) -> Result<AgentResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let id = cmd.id.map(|s| AgentId::from_str(&s)).transpose()?;
        let parent_id = cmd.parent_id.map(|s| AgentId::from_str(&s)).transpose()?;

        let domain_cmd = DomainRegisterAgent {
            org_id,
            project,
            namespace,
            roles: cmd.roles,
            description: cmd.description,
            id,
            parent_id,
            metadata: cmd.metadata,
        };

        if let Some(parent_id) = domain_cmd.parent_id {
            let parent = self
                .agents
                .find_by_id(&parent_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("agent {parent_id}")))?;

            if *parent.org_id() != domain_cmd.org_id {
                return Err(Error::InvalidInput(format!(
                    "parent agent {} belongs to org {}, expected {}",
                    parent_id,
                    parent.org_id(),
                    domain_cmd.org_id,
                )));
            }
            if *parent.project() != domain_cmd.project {
                return Err(Error::InvalidInput(format!(
                    "parent agent {} belongs to project {}, expected {}",
                    parent_id,
                    parent.project(),
                    domain_cmd.project,
                )));
            }

            let mut agent = orchy_core::agent::Agent::from_parent(
                &parent,
                domain_cmd.namespace,
                domain_cmd.roles,
                domain_cmd.description,
                domain_cmd.id,
            )?;
            self.agents.save(&mut agent).await?;
            return Ok(AgentResponse::from(&agent));
        }

        if let Some(ref id) = domain_cmd.id
            && let Some(mut existing) = self.agents.find_by_id(id).await?
            && *existing.org_id() == domain_cmd.org_id
            && *existing.project() == domain_cmd.project
        {
            existing.resume(
                domain_cmd.namespace,
                domain_cmd.roles,
                domain_cmd.description,
            )?;
            self.agents.save(&mut existing).await?;
            return Ok(AgentResponse::from(&existing));
        }

        let mut agent = orchy_core::agent::Agent::register(
            domain_cmd.org_id,
            domain_cmd.project,
            domain_cmd.namespace,
            domain_cmd.roles,
            domain_cmd.description,
            domain_cmd.id,
            domain_cmd.metadata,
        )?;
        self.agents.save(&mut agent).await?;
        Ok(AgentResponse::from(&agent))
    }
}
