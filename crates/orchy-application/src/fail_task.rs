use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct FailTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
}

pub struct FailTask {
    tasks: Arc<dyn TaskStore>,
}

impl FailTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: FailTaskCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.fail(cmd.reason)?;
        self.tasks.save(&mut task).await?;
        Ok(task)
    }
}
