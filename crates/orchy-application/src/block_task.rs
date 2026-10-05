use std::sync::Arc;

use orchy_core::{Clock, DomainError, Id, TaskStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::task_graph::TaskGraph;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
    pub on: Vec<String>,
}

pub struct BlockTask {
    tasks: Arc<dyn TaskStore>,
    graph: Arc<TaskGraph>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl BlockTask {
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

    pub async fn execute(&self, cmd: BlockTaskCommand) -> ApplicationResult<TaskDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: BlockTaskCommand) -> ApplicationResult<TaskDto> {
        if cmd.reason.is_none() && cmd.on.is_empty() {
            return Err(DomainError::validation(
                "say what blocks this: --on <task> for a dependency, or --reason for anything else",
            )
            .into());
        }

        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;

        let mut blockers = Vec::new();
        let mut adding = Vec::new();
        for blocker in &cmd.on {
            let id = self.tasks.require(&Id::new(blocker)?).await?.id().clone();
            task.add_dependency(id.clone(), &*self.clock)?;
            adding.push((task.id().clone(), id.clone()));
            blockers.push(id.to_string());
        }
        self.graph.ensure_no_loop(&adding, &[]).await?;

        let reason = cmd
            .reason
            .unwrap_or_else(|| format!("waiting on {}", blockers.join(", ")));
        task.block(reason, &*self.clock)?;

        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
