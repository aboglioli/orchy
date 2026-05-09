use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource, Result};
use orchy_core::graph::LinkParam;
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;

pub struct CompleteTaskCommand {
    pub task_id: String,
    pub org_id: String,
    pub summary: Option<String>,
    pub links: Vec<LinkParam>,
}

pub struct CompleteTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl CompleteTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: CompleteTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        if task.org_id() != &org_id {
            return Err(Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            }
            .into());
        }

        task.complete(cmd.summary)?;
        self.tasks.save(&mut task).await?;

        for link in cmd.links {
            let to_kind = link.to_kind.parse::<ResourceKind>()?;
            let rel_type = link.rel_type.parse::<RelationType>()?;
            let exists = self
                .edges
                .exists_by_pair(
                    &org_id,
                    &ResourceKind::Task,
                    &task_id.to_string(),
                    &to_kind,
                    &link.to_id,
                    &rel_type,
                )
                .await?;
            if !exists {
                let mut edge = Edge::new(
                    org_id.clone(),
                    ResourceKind::Task,
                    task_id.to_string(),
                    to_kind,
                    link.to_id,
                    rel_type,
                    None,
                )?;
                self.edges.save(&mut edge).await?;
            }
        }

        if let Err(e) = try_auto_complete_parent(&self.tasks, &self.edges, &org_id, &task_id).await
        {
            tracing::warn!("failed to check parent auto-complete for {task_id}: {e}");
        }

        if let Err(e) = self.try_unblock_dependents(&org_id, &task_id).await {
            tracing::warn!("failed to cascade-unblock dependents of {task_id}: {e}");
        }

        Ok(TaskDto::from(&task))
    }

    async fn try_unblock_dependents(&self, org: &OrganizationId, task_id: &TaskId) -> Result<()> {
        let dependent_edges = self
            .edges
            .find_to(
                org,
                &ResourceKind::Task,
                &task_id.to_string(),
                &[RelationType::DependsOn],
                None,
            )
            .await?;

        for edge in dependent_edges {
            let dependent_id: TaskId = match edge.from_id().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let Some(mut dependent) = self.tasks.find_by_id(&dependent_id).await? else {
                continue;
            };
            if dependent.status() != TaskStatus::Blocked {
                continue;
            }
            if !self.all_deps_completed(org, &dependent_id).await? {
                continue;
            }
            dependent.unblock()?;
            if let Err(e) = self.tasks.save(&mut dependent).await {
                tracing::warn!("failed to unblock dependent {dependent_id}: {e}");
            }
        }
        Ok(())
    }

    async fn all_deps_completed(&self, org: &OrganizationId, task_id: &TaskId) -> Result<bool> {
        let dep_edges = self
            .edges
            .find_from(
                org,
                &ResourceKind::Task,
                &task_id.to_string(),
                &[RelationType::DependsOn],
                None,
            )
            .await?;

        for edge in &dep_edges {
            let dep_id: TaskId = match edge.to_id().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };
            let Some(dep) = self.tasks.find_by_id(&dep_id).await? else {
                return Ok(false);
            };
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

pub(crate) async fn try_auto_complete_parent(
    tasks: &Arc<dyn TaskStore>,
    edges: &Arc<dyn EdgeStore>,
    org: &OrganizationId,
    task_id: &TaskId,
) -> Result<()> {
    let parent_edges = edges
        .find_to(
            org,
            &ResourceKind::Task,
            &task_id.to_string(),
            &[RelationType::Spawns],
            None,
        )
        .await?;
    let Some(parent_edge) = parent_edges.first() else {
        return Ok(());
    };
    let parent_id: TaskId = parent_edge
        .from_id()
        .parse()
        .map_err(|_| Error::invalid_input("invalid parent task id".to_owned()))?;

    let Some(mut parent) = tasks.find_by_id(&parent_id).await? else {
        return Ok(());
    };
    if parent.status() == TaskStatus::Completed {
        return Ok(());
    }

    let sibling_edges = edges
        .find_from(
            org,
            &ResourceKind::Task,
            &parent_id.to_string(),
            &[RelationType::Spawns],
            None,
        )
        .await?;
    let sibling_ids: Vec<TaskId> = sibling_edges
        .iter()
        .filter_map(|e| e.to_id().parse::<TaskId>().ok())
        .collect();
    let siblings = tasks.find_by_ids(&sibling_ids).await?;

    if Task::all_children_completed(&siblings) {
        parent.auto_complete("all subtasks completed".to_owned())?;
        if let Err(e) = tasks.save(&mut parent).await {
            tracing::warn!("failed to auto-complete parent {parent_id}: {e}");
        }
    }
    Ok(())
}
