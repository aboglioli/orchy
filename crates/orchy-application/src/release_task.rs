use std::sync::Arc;

use orchy_core::{ActorId, Clock, DomainError, Id, LeaseStore, ResourceKey, TaskStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReleaseTaskCommand {
    pub task_id: String,
    pub actor: String,
    /// Take the task back from whoever holds it, allowed only once their lease has expired.
    pub force: Option<String>,
}

pub struct ReleaseTask {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl ReleaseTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        leases: Arc<dyn LeaseStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            leases,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: ReleaseTaskCommand) -> ApplicationResult<TaskDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: ReleaseTaskCommand) -> ApplicationResult<TaskDto> {
        let id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;

        let mut task = self.tasks.require(&id).await?;
        let key = ResourceKey::task(&id);
        match cmd.force {
            None => task.release(&actor, &*self.clock)?,
            Some(reason) => {
                if let Some(lease) = self.leases.check(&key).await? {
                    return Err(DomainError::conflict(format!(
                        "{} still holds it until {}; wait for the lease to expire",
                        lease.holder(),
                        lease.expires_at()
                    ))
                    .into());
                }
                task.force_release(&actor, reason, &*self.clock)?;
            }
        }
        self.tasks.save(&mut task).await?;

        let _ = self.leases.release(&key, &actor).await;
        Ok(TaskDto::from(&task))
    }
}
