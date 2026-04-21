use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::agent::{AgentStore, Alias, validate_alias};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgeKind, KnowledgeStore};
use orchy_core::message::MessageStore;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

use crate::dto::{AgentResponse, RegisterAgentResponse};
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
    messages: Arc<dyn MessageStore>,
    tasks: Arc<dyn TaskStore>,
    knowledge: Arc<dyn KnowledgeStore>,
}

impl RegisterAgent {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        messages: Arc<dyn MessageStore>,
        tasks: Arc<dyn TaskStore>,
        knowledge: Arc<dyn KnowledgeStore>,
    ) -> Self {
        Self {
            agents,
            messages,
            tasks,
            knowledge,
        }
    }

    pub async fn execute(&self, cmd: RegisterAgentCommand) -> Result<RegisterAgentResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let alias = Alias::new(&cmd.alias)?;
        validate_alias(&cmd.alias)?;

        let mut agent = if let Some(mut existing) = self
            .agents
            .find_by_alias(&org_id, &project, &cmd.alias)
            .await?
        {
            existing.resume(
                namespace.clone(),
                cmd.roles.clone(),
                cmd.description.clone(),
            )?;
            if let Some(agent_type) = &cmd.agent_type {
                let mut meta = existing.metadata().clone();
                meta.insert("agent_type".to_string(), agent_type.clone());
                existing.set_metadata(meta)?;
            }
            if !cmd.metadata.is_empty() {
                let mut meta = existing.metadata().clone();
                meta.extend(cmd.metadata);
                existing.set_metadata(meta)?;
            }
            self.agents.save(&mut existing).await?;
            existing
        } else {
            let mut metadata = cmd.metadata;
            if let Some(agent_type) = &cmd.agent_type {
                metadata.insert("agent_type".to_string(), agent_type.clone());
            }
            let mut agent = orchy_core::agent::Agent::register(
                org_id.clone(),
                project.clone(),
                namespace.clone(),
                alias,
                cmd.roles,
                cmd.description,
                None,
                metadata,
            )?;
            self.agents.save(&mut agent).await?;
            agent
        };

        let agent_id = agent.id().clone();
        let agent_roles = agent.roles().to_vec();
        let agent_namespace = agent.namespace().clone();

        let inbox = self
            .messages
            .find_unread(
                &agent_id,
                &agent_roles,
                &agent_namespace,
                &org_id,
                &project,
                PageParams::unbounded(),
            )
            .await?;

        let pending_tasks = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project.clone()),
                    status: Some(TaskStatus::Pending),
                    assigned_to: None,
                    assigned_role: None,
                    tag: None,
                    namespace: Some(namespace.clone()),
                },
                PageParams::unbounded(),
            )
            .await?;

        let my_tasks = self
            .tasks
            .list(
                TaskFilter {
                    org_id: Some(org_id.clone()),
                    project: Some(project.clone()),
                    status: None,
                    assigned_to: Some(agent_id.clone()),
                    assigned_role: None,
                    tag: None,
                    namespace: Some(namespace.clone()),
                },
                PageParams::unbounded(),
            )
            .await?;

        let stale_tasks: Vec<_> = my_tasks.items.iter().filter(|t| t.is_stale()).collect();

        Ok(RegisterAgentResponse {
            agent: AgentResponse::from(&agent),
            inbox_count: inbox.items.len(),
            pending_tasks_count: pending_tasks.items.len(),
            my_tasks_count: my_tasks.items.len(),
            stale_tasks_count: stale_tasks.len(),
        })
    }
}
