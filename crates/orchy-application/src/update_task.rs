use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Priority, TaskId, TaskStore};

use crate::dto::TaskResponse;

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

    pub async fn execute(&self, cmd: UpdateTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let priority = cmd
            .priority
            .map(|p| p.parse::<Priority>())
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.update_details(
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
        )?;

        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
