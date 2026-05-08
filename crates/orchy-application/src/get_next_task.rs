use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::AgentId;
use orchy_core::error::{DomainError, Error, Resource, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;

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

    pub async fn execute(&self, cmd: GetNextTaskCommand) -> ApplicationResult<Option<TaskDto>> {
        let org_id = cmd.org_id.map(|s| OrganizationId::new(&s)).transpose()?;

        let project = cmd.project.map(ProjectId::try_from).transpose()?;

        let namespace = cmd.namespace.map(Namespace::new).transpose()?;

        let candidates = self
            .sorted_claimable_for_roles(&cmd.roles, org_id.clone(), project, namespace)
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
            .ok_or_else(|| Error::invalid_input("agent_id required when claiming"))?;

        for mut task in candidates {
            if !self.all_deps_completed(org_id.as_ref(), &task).await? {
                continue;
            }
            if matches!(task.status(), TaskStatus::Claimed | TaskStatus::InProgress)
                && task.release().is_err()
            {
                continue;
            }
            match task.claim(agent_id.clone()) {
                Ok(()) => {
                    self.tasks.save(&mut task).await?;
                    return Ok(Some(TaskDto::from(&task)));
                }
                Err(DomainError::InvalidTransition { .. }) => continue,
                Err(e) => return Err(e.into()),
            }
        }

        Ok(None)
    }

    async fn sorted_claimable_for_roles(
        &self,
        roles: &[String],
        org_id: Option<OrganizationId>,
        project: Option<ProjectId>,
        namespace: Option<Namespace>,
    ) -> Result<Vec<Task>> {
        let mut candidates: Vec<Task> = Vec::new();
        for status in [
            TaskStatus::Pending,
            TaskStatus::Claimed,
            TaskStatus::InProgress,
        ] {
            let page = self
                .tasks
                .list(
                    TaskFilter {
                        org_id: org_id.clone(),
                        project: project.clone(),
                        namespace: namespace.clone(),
                        status: Some(status),
                        include_archived: None,
                        ..Default::default()
                    },
                    PageParams::unbounded(),
                )
                .await?;
            candidates.extend(page.items);
        }

        candidates.retain(|task| task.can_be_claimed());

        candidates.retain(|task| {
            task.assigned_roles().is_empty()
                || roles.iter().any(|role| {
                    task.assigned_roles()
                        .iter()
                        .any(|assigned| assigned == role)
                })
        });

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
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Task,
                    id: dep_id.to_string(),
                })?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{Duration, Utc};
    use orchy_core::agent::AgentId;
    use orchy_core::namespace::{Namespace, ProjectId};
    use orchy_core::organization::OrganizationId;
    use orchy_core::task::{Priority, RestoreTask, Task, TaskId, TaskStatus, TaskStore};
    use orchy_store_memory::{MemoryEdgeStore, MemoryState, MemoryTaskStore};

    use super::GetNextTask;
    use crate::GetNextTaskCommand;

    fn make_stale_claimed_task(
        org_id: &OrganizationId,
        project: &ProjectId,
        agent: AgentId,
    ) -> Task {
        Task::restore(RestoreTask {
            id: TaskId::new(),
            org_id: org_id.clone(),
            project: project.clone(),
            namespace: Namespace::root(),
            title: "Stale Claimed Task".to_string(),
            description: "Abandoned by previous agent".to_string(),
            acceptance_criteria: None,
            status: TaskStatus::Claimed,
            priority: Priority::default(),
            assigned_roles: vec![],
            assigned_to: Some(agent),
            assigned_at: None,
            stale_after_secs: Some(1),
            last_activity_at: Utc::now() - Duration::minutes(5),
            tags: vec![],
            result_summary: None,
            archived_at: None,
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            version: 1,
        })
    }

    #[tokio::test]
    async fn get_next_task_returns_stale_claimed_task() {
        let state = Arc::new(MemoryState::new());
        let task_store = Arc::new(MemoryTaskStore::new(Arc::clone(&state)));
        let edge_store = Arc::new(MemoryEdgeStore::new(Arc::clone(&state)));

        let org_id = OrganizationId::new("test-org").unwrap();
        let project = ProjectId::try_from("test-project").unwrap();
        let original_agent = AgentId::new();
        let new_agent = AgentId::new();

        let stale_task = make_stale_claimed_task(&org_id, &project, original_agent);
        let task_id = stale_task.id();
        state.insert_task(stale_task);

        #[allow(clippy::clone_on_ref_ptr)]
        let use_case = GetNextTask::new(task_store.clone(), edge_store);
        let result = use_case
            .execute(GetNextTaskCommand {
                org_id: Some(org_id.to_string()),
                project: Some(project.to_string()),
                namespace: None,
                roles: vec![],
                claim: Some(true),
                agent_id: Some(new_agent.to_string()),
            })
            .await
            .unwrap();

        assert!(result.is_some());
        let dto = result.unwrap();
        assert_eq!(dto.id, task_id.to_string());
        assert_eq!(dto.status, "claimed");

        let updated = task_store.find_by_id(&task_id).await.unwrap().unwrap();
        assert_eq!(updated.status(), TaskStatus::Claimed);
        assert_eq!(updated.assigned_to(), Some(&new_agent));
    }
}
