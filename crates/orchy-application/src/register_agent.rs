use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::agent::{validate_alias, AgentStore};
use orchy_core::error::{Error, Result};

use crate::dto::AgentResponse;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct RegisterAgentCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub alias: String,
    pub roles: Vec<String>,
    pub description: String,
    pub agent_type: Option<String>,
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

        validate_alias(&cmd.alias)?;

        if let Some(mut existing) = self
            .agents
            .find_by_alias(&org_id, &project, &cmd.alias)
            .await?
        {
            existing.resume(namespace, cmd.roles.clone(), cmd.description.clone())?;
            if let Some(agent_type) = cmd.agent_type {
                let mut meta = existing.metadata().clone();
                meta.insert("agent_type".to_string(), agent_type);
                existing.set_metadata(meta)?;
            }
            if !cmd.metadata.is_empty() {
                let mut meta = existing.metadata().clone();
                meta.extend(cmd.metadata);
                existing.set_metadata(meta)?;
            }
            self.agents.save(&mut existing).await?;
            return Ok(AgentResponse::from(&existing));
        }

        let mut metadata = cmd.metadata;
        if let Some(agent_type) = cmd.agent_type {
            metadata.insert("agent_type".to_string(), agent_type);
        }
        let mut agent = orchy_core::agent::Agent::register(
            org_id,
            project,
            namespace,
            cmd.alias,
            cmd.roles,
            cmd.description,
            None,
            metadata,
        )?;
        self.agents.save(&mut agent).await?;
        Ok(AgentResponse::from(&agent))
    }
}
