use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::agent::{AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::message::MessageStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};
use orchy_core::user::UserId;
use std::str::FromStr;

use crate::dto::{AgentDto, RegisterAgentDto};
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
    pub auth_user_id: Option<String>,
}

pub struct RegisterAgent {
    agents: Arc<dyn AgentStore>,
    messages: Arc<dyn MessageStore>,
    tasks: Arc<dyn TaskStore>,
}

impl RegisterAgent {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        messages: Arc<dyn MessageStore>,
        tasks: Arc<dyn TaskStore>,
    ) -> Self {
        Self {
            agents,
            messages,
            tasks,
        }
    }

    pub async fn execute(&self, cmd: RegisterAgentCommand) -> Result<RegisterAgentDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let alias = Alias::new(&cmd.alias).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let auth_user_id = cmd
            .auth_user_id
            .as_deref()
            .map(UserId::from_str)
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid auth_user_id: {e}")))?;

        let agent = if let Some(mut existing) =
            self.agents.find_by_alias(&org_id, &project, &alias).await?
        {
            // Apply ownership resume conflict rule
            match (existing.user_id(), auth_user_id.as_ref()) {
                (Some(existing_uid), Some(provided_uid)) if existing_uid != provided_uid => {
                    return Err(Error::Conflict(format!(
                        "agent '{}' is owned by a different user",
                        cmd.alias
                    )));
                }
                (None, Some(provided_uid)) => {
                    existing.attach_user(*provided_uid);
                }
                _ => {}
            }

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
                auth_user_id,
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
                agent.user_id(),
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
                    include_archived: None,
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
                    include_archived: None,
                },
                PageParams::unbounded(),
            )
            .await?;

        let stale_tasks: Vec<_> = my_tasks.items.iter().filter(|t| t.is_stale()).collect();

        Ok(RegisterAgentDto {
            agent: AgentDto::from(&agent),
            inbox_count: inbox.items.len(),
            pending_tasks_count: pending_tasks.items.len(),
            my_tasks_count: my_tasks.items.len(),
            stale_tasks_count: stale_tasks.len(),
        })
    }
}
