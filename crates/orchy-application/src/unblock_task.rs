use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct UnblockTaskCommand {
    pub task_id: String,
}

pub struct UnblockTask {
    tasks: Arc<dyn TaskStore>,
}

impl UnblockTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: UnblockTaskCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.unblock()?;
        self.tasks.save(&mut task).await?;
        Ok(task)
    }
}
