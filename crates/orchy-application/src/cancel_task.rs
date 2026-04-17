use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct CancelTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
}

pub struct CancelTask {
    tasks: Arc<dyn TaskStore>,
}

impl CancelTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: CancelTaskCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.cancel(cmd.reason)?;
        self.tasks.save(&mut task).await?;
        Ok(task)
    }
}
