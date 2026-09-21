use std::collections::HashSet;
use std::sync::Arc;

use chrono::Duration;
use orchy_core::task::rollup;
use orchy_core::{ActorId, Clock, Id, LeaseStore, ResourceKey, Task, TaskStore};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

const GUARD_TTL_SECONDS: i64 = 30;

pub struct RollupAncestors {
    tasks: Arc<dyn TaskStore>,
    leases: Arc<dyn LeaseStore>,
    clock: Arc<dyn Clock>,
}

impl RollupAncestors {
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

    pub async fn execute(&self, from: &Id, by: &ActorId) -> ApplicationResult<Vec<TaskDto>> {
        let mut seen = HashSet::new();
        let mut changed = Vec::new();
        let mut cursor = self.tasks.require(from).await?.parent().cloned();

        while let Some(parent_id) = cursor {
            if !seen.insert(parent_id.clone()) || seen.len() > rollup::MAX_DEPTH {
                break;
            }
            match self.roll_up_one(&parent_id, by).await? {
                Some(parent) => {
                    cursor = parent.parent().cloned();
                    changed.push(TaskDto::from(&parent));
                }
                None => break,
            }
        }
        Ok(changed)
    }

    /// Deriving a parent is read, decide, write, and two children finishing at the same moment
    /// would otherwise both see an open parent and both move it. The guard is keyed apart from
    /// the task's own lease so it never collides with an agent's claim on that task, and the
    /// parent is re-read inside it because the answer may have changed while waiting.
    async fn roll_up_one(&self, parent_id: &Id, by: &ActorId) -> ApplicationResult<Option<Task>> {
        let guard = ResourceKey::new(format!("rollup:{parent_id}"))?;
        if self
            .leases
            .acquire(&guard, by, Duration::seconds(GUARD_TTL_SECONDS))
            .await
            .is_err()
        {
            // somebody else is deriving this very parent; their answer will be ours
            return Ok(None);
        }

        let derived = self.derive(parent_id).await;
        let _ = self.leases.release(&guard, by).await;
        derived
    }

    async fn derive(&self, parent_id: &Id) -> ApplicationResult<Option<Task>> {
        let Some(mut parent) = self.tasks.get(parent_id).await? else {
            return Ok(None);
        };
        let children = self.tasks.children_of(parent_id).await?;
        let statuses: Vec<_> = children.iter().map(Task::status).collect();

        let Some(next) = rollup::resolve(&statuses) else {
            return Ok(None);
        };
        if !parent.roll_up(next, "all subtasks are terminal".to_owned(), &*self.clock) {
            return Ok(None);
        }
        self.tasks.save(&mut parent).await?;
        Ok(Some(parent))
    }
}
