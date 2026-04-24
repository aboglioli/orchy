use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;

pub struct AddDependencyCommand {
    pub org_id: String,
    pub task_id: String,
    pub dependency_id: String,
}

pub struct AddDependency {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl AddDependency {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: AddDependencyCommand) -> Result<TaskDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let dependency_id = cmd.dependency_id.parse::<TaskId>()?;

        self.tasks
            .find_by_id(&dependency_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("dependency task {dependency_id}")))?;

        if task_id == dependency_id {
            return Err(Error::Conflict(format!(
                "task {task_id} cannot depend on itself"
            )));
        }

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        if matches!(
            task.status(),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot add dependency to task {} with status {}",
                task_id,
                task.status()
            )));
        }

        let already_exists = self
            .edges
            .exists_by_pair(
                &org_id,
                &ResourceKind::Task,
                &task_id.to_string(),
                &ResourceKind::Task,
                &dependency_id.to_string(),
                &RelationType::DependsOn,
            )
            .await?;

        if already_exists {
            return Ok(TaskDto::from(&task));
        }

        let mut edge = Edge::new(
            org_id.clone(),
            ResourceKind::Task,
            task_id.to_string(),
            ResourceKind::Task,
            dependency_id.to_string(),
            RelationType::DependsOn,
            None,
        )?;
        self.edges.save(&mut edge).await?;

        if !self.all_deps_completed(&org_id, &task_id).await?
            && task.status() == TaskStatus::Pending
        {
            task.block()?;
            self.tasks.save(&mut task).await?;
        }

        Ok(TaskDto::from(&task))
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
