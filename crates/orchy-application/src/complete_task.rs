use std::sync::Arc;

use orchy_core::{ActorId, Clock, Id, LeaseStore, ResourceKey, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CompleteTaskCommand {
    pub task_id: String,
    pub note: Option<String>,
    pub actor: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteTaskResponse {
    pub task: TaskDto,
    pub ancestors: Vec<TaskDto>,
}

pub struct CompleteTask {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    rollup: Arc<RollupAncestors>,
    clock: Arc<dyn Clock>,
}

impl CompleteTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        leases: Arc<dyn LeaseStore>,
        rollup: Arc<RollupAncestors>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            leases,
            rollup,
            clock,
        }
    }

    pub async fn execute(
        &self,
        cmd: CompleteTaskCommand,
    ) -> ApplicationResult<CompleteTaskResponse> {
        let id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;

        let mut task = self.tasks.require(&id).await?;
        task.complete(&actor, cmd.note, &*self.clock)?;
        self.tasks.save(&mut task).await?;

        let _ = self.leases.release(&ResourceKey::task(&id), &actor).await;
        let ancestors = self.rollup.execute(&id).await?;

        Ok(CompleteTaskResponse {
            task: TaskDto::from(&task),
            ancestors,
        })
    }
}
