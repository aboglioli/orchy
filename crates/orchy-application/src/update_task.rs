use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::task::{Priority, TaskId, TaskStore};

use crate::dto::TaskDto;

pub struct UpdateTaskCommand {
    pub task_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
}

pub struct UpdateTask {
    tasks: Arc<dyn TaskStore>,
}

impl UpdateTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: UpdateTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let priority = cmd.priority.map(|p| p.parse::<Priority>()).transpose()?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        task.update_details(
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
        )?;

        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
