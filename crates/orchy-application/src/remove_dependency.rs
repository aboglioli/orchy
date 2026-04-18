use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct RemoveDependencyCommand {
    pub task_id: String,
    pub dependency_id: String,
}

pub struct RemoveDependency {
    tasks: Arc<dyn TaskStore>,
}

impl RemoveDependency {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: RemoveDependencyCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let dependency_id = cmd
            .dependency_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.remove_dependency(&dependency_id)?;

        if task.status() == TaskStatus::Blocked
            && self.all_deps_completed(task.depends_on()).await?
        {
            task.unblock()?;
        }

        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
        for dep_id in deps {
            let dep = self
                .tasks
                .find_by_id(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
