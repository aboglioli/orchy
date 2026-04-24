use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::parse_namespace;

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

    pub async fn execute(&self, cmd: MoveTaskCommand) -> Result<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let namespace = parse_namespace(Some(&cmd.new_namespace))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.move_to(namespace)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
