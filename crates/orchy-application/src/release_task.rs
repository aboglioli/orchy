use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct ReleaseTaskCommand {
    pub task_id: String,
}

pub struct ReleaseTask {
    tasks: Arc<dyn TaskStore>,
}

impl ReleaseTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ReleaseTaskCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.release()?;
        self.tasks.save(&mut task).await?;
        Ok(task)
    }
}
