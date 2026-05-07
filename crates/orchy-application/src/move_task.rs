use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::namespace::Namespace;
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;

pub struct MoveTaskCommand {
    pub task_id: String,
    pub new_namespace: String,
}

pub struct MoveTask {
    tasks: Arc<dyn TaskStore>,
}

impl MoveTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: MoveTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let namespace = Namespace::new(cmd.new_namespace.as_str())?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        task.move_to(namespace)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
