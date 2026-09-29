use std::sync::Arc;

use orchy_core::task::dependencies::Outcome;
use orchy_core::{EdgeStore, EntityRef, Id, TaskStore};
use serde::{Deserialize, Serialize};

use crate::assess_dependencies::{AssessDependencies, DependencyDto};
use crate::dto::{EdgeDto, TaskDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GetTaskCommand {
    pub task_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetTaskResponse {
    pub task: TaskDto,
    pub subtasks: Vec<TaskDto>,
    pub edges: Vec<EdgeDto>,
    pub dependencies: Vec<DependencyDto>,
    pub readiness: Outcome,
}

pub struct GetTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    dependencies: Arc<AssessDependencies>,
}

impl GetTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        dependencies: Arc<AssessDependencies>,
    ) -> Self {
        Self {
            tasks,
            edges,
            dependencies,
        }
    }

    pub async fn execute(&self, cmd: GetTaskCommand) -> ApplicationResult<GetTaskResponse> {
        let id = Id::new(&cmd.task_id)?;
        let task = self.tasks.require(&id).await?;
        let subtasks = self.tasks.children_of(&id).await?;
        let edges = self.edges.out(&EntityRef::task(id), None).await?;
        let dependencies = self.dependencies.each(&task).await?;
        let readiness = self.dependencies.outcome(&task).await?;

        Ok(GetTaskResponse {
            task: TaskDto::from(&task),
            subtasks: subtasks.iter().map(TaskDto::from).collect(),
            edges: edges.iter().map(EdgeDto::from).collect(),
            dependencies,
            readiness,
        })
    }
}
