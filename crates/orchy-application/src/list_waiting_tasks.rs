use std::sync::Arc;

use orchy_core::task::dependencies::Outcome;
use orchy_core::{Namespace, TaskQuery, TaskStatus, TaskStore};
use serde::{Deserialize, Serialize};

use crate::assess_dependencies::{AssessDependencies, DependencyDto};
use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListWaitingTasksCommand {
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaitingTaskDto {
    pub task: TaskDto,
    pub waiting_on: Vec<DependencyDto>,
}

/// Open work that `task next` will not hand out: blocked tasks, and pending ones whose
/// dependencies are not all satisfied.
pub struct ListWaitingTasks {
    tasks: Arc<dyn TaskStore>,
    dependencies: Arc<AssessDependencies>,
}

impl ListWaitingTasks {
    pub fn new(tasks: Arc<dyn TaskStore>, dependencies: Arc<AssessDependencies>) -> Self {
        Self {
            tasks,
            dependencies,
        }
    }

    pub async fn execute(
        &self,
        cmd: ListWaitingTasksCommand,
    ) -> ApplicationResult<Vec<WaitingTaskDto>> {
        let open = self
            .tasks
            .matching(&TaskQuery {
                status: Some(vec![TaskStatus::Pending, TaskStatus::Blocked]),
                namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
                ..Default::default()
            })
            .await?;

        let mut waiting = Vec::new();
        for task in open {
            let readiness = self.dependencies.outcome(&task).await?;
            if task.status() == TaskStatus::Pending && readiness == Outcome::Satisfied {
                continue;
            }
            let waiting_on = self
                .dependencies
                .each(&task)
                .await?
                .into_iter()
                .filter(|d| d.outcome != Outcome::Satisfied)
                .collect();
            waiting.push(WaitingTaskDto {
                task: TaskDto::from(&task),
                waiting_on,
            });
        }
        Ok(waiting)
    }
}
