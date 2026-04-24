use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;

pub struct ArchiveTaskCommand {
    pub org_id: String,
    pub task_id: String,
    pub reason: Option<String>,
}

pub struct ArchiveTask {
    tasks: Arc<dyn TaskStore>,
}

impl ArchiveTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ArchiveTaskCommand) -> Result<TaskDto> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.archive(cmd.reason)?;
        self.tasks.save(&mut task).await?;

        Ok(TaskDto::from(&task))
    }
}
