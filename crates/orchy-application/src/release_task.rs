use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;

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

    pub async fn execute(&self, cmd: ReleaseTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        task.release()?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
