use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct GetTaskCommand {
    pub task_id: String,
}

pub struct GetTask {
    tasks: Arc<dyn TaskStore>,
}

impl GetTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: GetTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;
        Ok(TaskResponse::from(task))
    }
}
