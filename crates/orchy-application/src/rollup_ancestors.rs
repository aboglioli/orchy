use std::collections::HashSet;
use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{Clock, DomainError, Id, Task, TaskStore};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

const DERIVE_ATTEMPTS: u32 = 4;

pub struct RollupAncestors {
    tasks: Arc<dyn TaskStore>,
    clock: Arc<dyn Clock>,
}

impl RollupAncestors {
    pub fn new(tasks: Arc<dyn TaskStore>, clock: Arc<dyn Clock>) -> Self {
        Self { tasks, clock }
    }

    pub async fn execute(&self, from: &Id) -> ApplicationResult<Vec<TaskDto>> {
        let mut seen = HashSet::new();
        let mut changed = Vec::new();
        let mut cursor = self.tasks.require(from).await?.parent().cloned();

        while let Some(parent_id) = cursor {
            if !seen.insert(parent_id.clone()) || seen.len() > rollup::MAX_DEPTH {
                break;
            }
            match self.derive(&parent_id).await? {
                Some(parent) => {
                    cursor = parent.parent().cloned();
                    changed.push(TaskDto::from(&parent));
                }
                None => break,
            }
        }
        Ok(changed)
    }

    /// Every child that finishes derives the parent, so two finishing together both try to move
    /// it. No lease, because one that is not granted would have to be waited for: the holder
    /// may have read the children before this one was saved. A rollup is a pure function of the
    /// children, so a lost write just means deciding again on a parent that is terminal by then.
    async fn derive(&self, parent_id: &Id) -> ApplicationResult<Option<Task>> {
        for _ in 0..DERIVE_ATTEMPTS {
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
            match self.tasks.save(&mut parent).await {
                Err(DomainError::Conflict(_)) => continue,
                Err(e) => return Err(e.into()),
                Ok(()) => return Ok(Some(parent)),
            }
        }
        Ok(None)
    }
}
