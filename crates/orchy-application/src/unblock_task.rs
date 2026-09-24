use std::sync::Arc;

use orchy_core::{Clock, Id, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UnblockTaskCommand {
    pub task_id: String,
}

pub struct UnblockTask {
    tasks: Arc<dyn TaskStore>,
    clock: Arc<dyn Clock>,
}

impl UnblockTask {
    pub fn new(tasks: Arc<dyn TaskStore>, clock: Arc<dyn Clock>) -> Self {
        Self { tasks, clock }
    }

    pub async fn execute(&self, cmd: UnblockTaskCommand) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;
        task.unblock(&*self.clock)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
