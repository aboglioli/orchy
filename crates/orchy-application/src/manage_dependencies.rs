use std::sync::Arc;

use orchy_core::{Clock, Id, TaskStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManageDependenciesCommand {
    pub task_id: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

pub struct ManageDependencies {
    tasks: Arc<dyn TaskStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl ManageDependencies {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: ManageDependenciesCommand) -> ApplicationResult<TaskDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: ManageDependenciesCommand) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;
        for dependency in &cmd.add {
            task.add_dependency(Id::new(dependency)?, &*self.clock)?;
        }
        for dependency in &cmd.remove {
            task.remove_dependency(&Id::new(dependency)?, &*self.clock);
        }
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
