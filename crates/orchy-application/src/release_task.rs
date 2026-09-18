use std::sync::Arc;

use orchy_core::{ActorId, Clock, Id, LeaseStore, ResourceKey, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReleaseTaskCommand {
    pub task_id: String,
    pub actor: String,
}

pub struct ReleaseTask {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    clock: Arc<dyn Clock>,
}

impl ReleaseTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        leases: Arc<dyn LeaseStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            leases,
            clock,
        }
    }

    pub async fn execute(&self, cmd: ReleaseTaskCommand) -> ApplicationResult<TaskDto> {
        let id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;

        let mut task = self.tasks.require(&id).await?;
        task.release(&actor, &*self.clock)?;
        self.tasks.save(&mut task).await?;

        let _ = self.leases.release(&ResourceKey::task(&id), &actor).await;
        Ok(TaskDto::from(&task))
    }
}
