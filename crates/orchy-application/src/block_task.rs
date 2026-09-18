use std::sync::Arc;

use orchy_core::{Clock, Id, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockTaskCommand {
    pub task_id: String,
    pub reason: String,
}

pub struct BlockTask {
    tasks: Arc<dyn TaskStore>,
    clock: Arc<dyn Clock>,
}

impl BlockTask {
    pub fn new(tasks: Arc<dyn TaskStore>, clock: Arc<dyn Clock>) -> Self {
        Self { tasks, clock }
    }

    pub async fn execute(&self, cmd: BlockTaskCommand) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;
        task.block(cmd.reason, &*self.clock)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
