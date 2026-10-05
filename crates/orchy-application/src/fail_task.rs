use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{
    ActorId, Clock, Id, LeaseStore, ResourceKey, Task, TaskStatus, TaskStore, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::complete_task::CompleteTaskResponse;
use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rollup_ancestors::RollupAncestors;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FailTaskCommand {
    pub task_id: String,
    pub reason: String,
    pub actor: String,
}

pub struct FailTask {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    rollup: Arc<RollupAncestors>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl FailTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        leases: Arc<dyn LeaseStore>,
        rollup: Arc<RollupAncestors>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            leases,
            rollup,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: FailTaskCommand) -> ApplicationResult<CompleteTaskResponse> {
        let finished = atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await?;
        if let (Ok(id), Ok(actor)) = (Id::new(&cmd.task_id), cmd.actor.parse::<ActorId>()) {
            let _ = self.leases.release(&ResourceKey::task(&id), &actor).await;
        }
        Ok(finished)
    }

    async fn apply(&self, cmd: FailTaskCommand) -> ApplicationResult<CompleteTaskResponse> {
        let id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;

        let mut task = self.tasks.require(&id).await?;
        let children: Vec<TaskStatus> = self
            .tasks
            .children_of(&id)
            .await?
            .iter()
            .map(Task::status)
            .collect();
        rollup::ensure_can_finish(&children)?;
        task.fail(&actor, cmd.reason, &*self.clock)?;
        self.tasks.save(&mut task).await?;

        let ancestors = self.rollup.execute(&id).await?;

        Ok(CompleteTaskResponse {
            task: TaskDto::from(&task),
            ancestors,
        })
    }
}
