use std::sync::Arc;

use orchy_core::{Clock, Id, TaskStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::task_graph::TaskGraph;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManageDependenciesCommand {
    pub task_id: String,
    pub add: Vec<String>,
    pub remove: Vec<String>,
}

pub struct ManageDependencies {
    tasks: Arc<dyn TaskStore>,
    graph: Arc<TaskGraph>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl ManageDependencies {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        graph: Arc<TaskGraph>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            graph,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: ManageDependenciesCommand) -> ApplicationResult<TaskDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: ManageDependenciesCommand) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;
        let mut adding = Vec::new();
        for dependency in &cmd.add {
            let dependency = self.tasks.require(&Id::new(dependency)?).await?;
            task.add_dependency(dependency.id().clone(), &*self.clock)?;
            adding.push((task.id().clone(), dependency.id().clone()));
        }
        let mut removing = Vec::new();
        for dependency in &cmd.remove {
            let dependency = Id::new(dependency)?;
            task.remove_dependency(&dependency, &*self.clock);
            removing.push((task.id().clone(), dependency));
        }
        self.graph.ensure_no_loop(&adding, &removing).await?;
        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
