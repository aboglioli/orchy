use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
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

    pub async fn execute(&self, cmd: ArchiveTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        task.archive(cmd.reason)?;
        self.tasks.save(&mut task).await?;

        Ok(TaskDto::from(&task))
    }
}
