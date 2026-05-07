use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;

pub struct TagTaskCommand {
    pub task_id: String,
    pub tag: String,
}

pub struct TagTask {
    tasks: Arc<dyn TaskStore>,
}

impl TagTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: TagTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        task.add_tag(cmd.tag)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
