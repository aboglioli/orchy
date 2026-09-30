use std::sync::Arc;

use chrono::Duration;
use orchy_core::task::rollup;
use orchy_core::{ActorId, Clock, Id, LeaseStore, ResourceKey, Task, TaskStatus, TaskStore};

use crate::assess_dependencies::AssessDependencies;
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
    dependencies: Arc<AssessDependencies>,
    clock: Arc<dyn Clock>,
}

impl ClaimTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        leases: Arc<dyn LeaseStore>,
        dependencies: Arc<AssessDependencies>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            tasks,
            leases,
            dependencies,
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

        match self.take(&id, &actor, cmd.start).await {
            Ok(task) => Ok(task),
            Err(e) => {
                let _ = self.leases.release(&ResourceKey::task(&id), &actor).await;
                Err(e)
            }
        }
    }

    async fn take(&self, id: &Id, actor: &ActorId, start: bool) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(id).await?;
        let children: Vec<TaskStatus> = self
            .tasks
            .children_of(id)
            .await?
            .iter()
            .map(Task::status)
            .collect();
        rollup::ensure_claimable(&children)?;
        self.dependencies.outcome(&task).await?.ensure_claimable()?;
        task.claim(actor.clone(), &*self.clock)?;
        if start {
            task.start(&*self.clock)?;
        }
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
