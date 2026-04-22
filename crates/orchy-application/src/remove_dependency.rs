use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct RemoveDependencyCommand {
    pub org_id: String,
    pub task_id: String,
    pub dependency_id: String,
}

pub struct RemoveDependency {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl RemoveDependency {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: RemoveDependencyCommand) -> Result<TaskResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let dependency_id = cmd.dependency_id.parse::<TaskId>()?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let dep_edges = self
            .edges
            .find_from(
                &org_id,
                &ResourceKind::Task,
                &task_id.to_string(),
                &[RelationType::DependsOn],
                None,
            )
            .await?;
        if let Some(mut dep_edge) = dep_edges
            .into_iter()
            .find(|e| e.to_id() == dependency_id.to_string() && e.is_active())
        {
            dep_edge.invalidate()?;
            self.edges.save(&mut dep_edge).await?;
        }

        if task.status() == TaskStatus::Blocked
            && self.all_deps_completed(&org_id, &task_id).await?
        {
            task.unblock()?;
            self.tasks.save(&mut task).await?;
        }

        Ok(TaskResponse::from(&task))
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
