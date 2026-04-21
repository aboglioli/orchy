use std::str::FromStr;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct TouchTaskCommand {
    pub task_id: String,
    pub agent_id: Option<String>,
}

pub struct TouchTask {
    tasks: Arc<dyn TaskStore>,
}

impl TouchTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: TouchTaskCommand) -> Result<TaskResponse> {
        let task_id = TaskId::from_str(&cmd.task_id)?;
        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.touch();
        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
