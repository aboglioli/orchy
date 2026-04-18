use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::resource_ref::{ResourceKind, ResourceRef};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct RemoveTaskRefCommand {
    pub task_id: String,
    pub ref_kind: String,
    pub ref_id: String,
}

pub struct RemoveTaskRef {
    tasks: Arc<dyn TaskStore>,
}

impl RemoveTaskRef {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: RemoveTaskRefCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let kind = cmd
            .ref_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;

        let r = ResourceRef::new(kind, cmd.ref_id);

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.remove_ref(&r);
        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
