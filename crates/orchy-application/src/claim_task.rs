use std::sync::Arc;

use chrono::Duration;
use orchy_core::{ActorId, Clock, Id, LeaseStore, ResourceKey, TaskStore};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

const DEFAULT_LEASE_SECS: i64 = 900;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaimTaskCommand {
    pub task_id: String,
    pub actor: String,
    pub ttl_seconds: Option<i64>,
    pub start: bool,
}

pub struct ClaimTask {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    clock: Arc<dyn Clock>,
}

impl ClaimTask {
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

    pub async fn execute(&self, cmd: ClaimTaskCommand) -> ApplicationResult<TaskDto> {
        let id = Id::new(&cmd.task_id)?;
        let actor: ActorId = cmd.actor.parse()?;
        let ttl = Duration::seconds(cmd.ttl_seconds.unwrap_or(DEFAULT_LEASE_SECS));

        self.leases
            .acquire(&ResourceKey::task(&id), &actor, ttl)
            .await?;

        let mut task = self.tasks.require(&id).await?;
        match task.claim(actor.clone(), &*self.clock) {
            Ok(()) => {}
            Err(e) => {
                let _ = self.leases.release(&ResourceKey::task(&id), &actor).await;
                return Err(e.into());
            }
        }
        if cmd.start {
            task.start(&*self.clock)?;
        }
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
