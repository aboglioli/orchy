use std::sync::Arc;

use orchy_core::{Clock, DomainError, Id, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
    pub on: Vec<String>,
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
        if cmd.reason.is_none() && cmd.on.is_empty() {
            return Err(DomainError::validation(
                "say what blocks this: --on <task> for a dependency, or --reason for anything else",
            )
            .into());
        }

        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;

        let mut blockers = Vec::new();
        for blocker in &cmd.on {
            let id = Id::new(blocker)?;
            self.tasks.require(&id).await?;
            task.add_dependency(id.clone(), &*self.clock)?;
            blockers.push(id.to_string());
        }

        let reason = cmd
            .reason
            .unwrap_or_else(|| format!("waiting on {}", blockers.join(", ")));
        task.block(reason, &*self.clock)?;

        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
