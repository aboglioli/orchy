use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;
use crate::parse_namespace;

pub struct GetNextTaskCommand {
    pub org_id: Option<String>,
    pub project: Option<String>,
    pub namespace: Option<String>,
    pub roles: Vec<String>,
    pub claim: Option<bool>,
    pub agent_id: Option<String>,
}

pub struct GetNextTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl GetNextTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: GetNextTaskCommand) -> Result<Option<TaskDto>> {
        let org_id = cmd
            .org_id
            .map(|s| OrganizationId::new(&s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let namespace = cmd
            .namespace
            .map(|s| parse_namespace(Some(&s)))
            .transpose()?;

        let candidates = self
            .sorted_pending_for_roles(&cmd.roles, org_id.clone(), project, namespace)
            .await?;

        let should_claim = cmd.claim.unwrap_or(true);

        if !should_claim {
            for task in candidates {
                if self.all_deps_completed(org_id.as_ref(), &task).await? {
                    return Ok(Some(TaskDto::from(&task)));
                }
            }
            return Ok(None);
        }

        let agent_id = cmd
            .agent_id
            .map(|s| AgentId::from_str(&s))
            .transpose()?
            .ok_or_else(|| Error::InvalidInput("agent_id required when claiming".into()))?;

        for mut task in candidates {
            if self.all_deps_completed(org_id.as_ref(), &task).await? {
                match task.claim(agent_id.clone()) {
                    Ok(()) => {
                        self.tasks.save(&mut task).await?;
                        return Ok(Some(TaskDto::from(&task)));
                    }
                    Err(Error::InvalidTransition { .. }) => continue,
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(None)
    }

    async fn sorted_pending_for_roles(
        &self,
        roles: &[String],
        org_id: Option<OrganizationId>,
        project: Option<ProjectId>,
        namespace: Option<Namespace>,
    ) -> Result<Vec<Task>> {
        let mut candidates: Vec<Task> = self
            .tasks
            .list(
                TaskFilter {
                    org_id: org_id.clone(),
                    project: project.clone(),
                    namespace: namespace.clone(),
                    status: Some(TaskStatus::Pending),
                    include_archived: None,
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items
            .into_iter()
            .filter(|task| {
                task.assigned_roles().is_empty()
                    || roles.iter().any(|role| {
                        task.assigned_roles()
                            .iter()
                            .any(|assigned| assigned == role)
                    })
            })
            .collect();

        let mut seen = HashSet::new();
        candidates.retain(|t| seen.insert(t.id()));
        candidates.sort_by_key(|t| std::cmp::Reverse(t.priority()));
        Ok(candidates)
    }

    async fn all_deps_completed(
        &self,
        org_id: Option<&OrganizationId>,
        task: &Task,
    ) -> Result<bool> {
        let Some(org) = org_id else {
            return Ok(true);
        };
        let dep_edges = self
            .edges
            .find_from(
                org,
                &ResourceKind::Task,
                &task.id().to_string(),
                &[RelationType::DependsOn],
                None,
            )
            .await?;

        for edge in &dep_edges {
            let dep_id: TaskId = match edge.to_id().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };
            let dep = self
                .tasks
                .find_by_id(&dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
