use std::collections::HashSet;
use std::sync::Arc;

use orchy_core::task::rollup;
use orchy_core::{Clock, Id, Task, TaskStore};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

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
            let Some(mut parent) = self.tasks.get(&parent_id).await? else {
                break;
            };
            let children = self.tasks.children_of(&parent_id).await?;
            let statuses: Vec<_> = children.iter().map(Task::status).collect();

            let Some(next) = rollup::resolve(&statuses) else {
                break;
            };
            if !parent.roll_up(next, "all subtasks are terminal".to_owned(), &*self.clock) {
                break;
            }
            self.tasks.save(&mut parent).await?;
            changed.push(TaskDto::from(&parent));
            cursor = parent.parent().cloned();
        }
        Ok(changed)
    }
}
