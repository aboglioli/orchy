use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Resource};

use crate::dto::AgentDto;
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::project::ProjectStore;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::{TaskFilter, TaskStore};

pub struct SwitchContextCommand {
    pub org_id: String,
    pub agent_id: String,
    pub project: Option<String>,
    pub namespace: Option<String>,
}

pub struct SwitchContext {
    agents: Arc<dyn AgentStore>,
    projects: Arc<dyn ProjectStore>,
    tasks: Arc<dyn TaskStore>,
    locks: Arc<dyn LockStore>,
}

impl SwitchContext {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        projects: Arc<dyn ProjectStore>,
        tasks: Arc<dyn TaskStore>,
        locks: Arc<dyn LockStore>,
    ) -> Self {
        Self {
            agents,
            projects,
            tasks,
            locks,
        }
    }

    pub async fn execute(&self, cmd: SwitchContextCommand) -> ApplicationResult<AgentDto> {
        if cmd.project.is_none() && cmd.namespace.is_none() {
            return Err(Error::invalid_input(
                "at least one of project or namespace is required".to_string(),
            )
            .into());
        }

        let org = OrganizationId::new(&cmd.org_id)?;
        let agent_id = AgentId::from_str(&cmd.agent_id)?;

        let target_project = cmd.project.map(ProjectId::try_from).transpose()?;

        let mut agent =
            self.agents
                .find_by_id(&agent_id)
                .await?
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Agent,
                    id: agent_id.to_string(),
                })?;

        let current_project = agent.project().clone();

        let project_changed = target_project
            .as_ref()
            .is_some_and(|p| *p != current_project);

        let target_namespace = match &cmd.namespace {
            Some(ns) => Namespace::new(ns)?,
            None if project_changed => Namespace::root(),
            None => agent.namespace().clone(),
        };

        if let Some(ref p) = target_project {
            self.projects
                .find_by_id(&org, p)
                .await?
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Project,
                    id: p.to_string(),
                })?;
        }

        if project_changed {
            self.release_project_resources(&agent_id, &org, &current_project)
                .await;
        }

        agent.switch_context(target_project, target_namespace)?;
        self.agents.save(&mut agent).await?;
        Ok(AgentDto::from(&agent))
    }

    async fn release_project_resources(
        &self,
        agent_id: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
    ) {
        let tasks = self
            .tasks
            .list(
                TaskFilter {
                    assigned_to: Some(agent_id.clone()),
                    project: Some(project.clone()),
                    include_archived: None,
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await
            .map(|p| p.items)
            .unwrap_or_default();
        for mut task in tasks {
            let _ = task.release();
            let _ = self.tasks.save(&mut task).await;
        }

        let locks = self
            .locks
            .find_by_holder(agent_id, org)
            .await
            .unwrap_or_default();
        for lock in locks {
            if *lock.project() == *project {
                let _ = self
                    .locks
                    .delete(lock.org_id(), lock.project(), lock.namespace(), lock.name())
                    .await;
            }
        }
    }
}
