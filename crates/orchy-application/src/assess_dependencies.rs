use std::sync::Arc;

use orchy_core::task::dependencies::{self, Outcome};
use orchy_core::task::rollup;
use orchy_core::{EdgeStore, EntityKind, EntityRef, Id, Relation, Task, TaskStatus, TaskStore};
use serde::{Deserialize, Serialize};

use crate::error::ApplicationResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyDto {
    pub id: String,
    pub outcome: Outcome,
}

/// Decides whether a task's dependencies let it be worked on, following a superseded
/// dependency to whatever replaced it.
pub struct AssessDependencies {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl AssessDependencies {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn outcome(&self, task: &Task) -> ApplicationResult<Outcome> {
        let each = self.each(task).await?;
        let outcomes: Vec<Outcome> = each.iter().map(|d| d.outcome).collect();
        Ok(dependencies::combine(&outcomes))
    }

    /// Every dependency with its own outcome, in the order the task lists them.
    pub async fn each(&self, task: &Task) -> ApplicationResult<Vec<DependencyDto>> {
        let mut assessed = Vec::new();
        for dependency in task.depends_on() {
            assessed.push(DependencyDto {
                id: dependency.to_string(),
                outcome: self.of(dependency).await?,
            });
        }
        Ok(assessed)
    }

    async fn of(&self, id: &Id) -> ApplicationResult<Outcome> {
        let mut path = vec![id.clone()];
        self.follow(id, &mut path).await
    }

    async fn follow(&self, id: &Id, path: &mut Vec<Id>) -> ApplicationResult<Outcome> {
        let status = self.tasks.get(id).await?.map(|t| t.status());
        let mut replacements = Vec::new();
        if status == Some(TaskStatus::Superseded) && path.len() < rollup::MAX_DEPTH {
            let replaced_by = self
                .edges
                .incoming(&EntityRef::task(id.clone()), Some(&Relation::Supersedes))
                .await?;
            for edge in replaced_by {
                let from = edge.from();
                if from.kind() != EntityKind::Task || path.contains(from.id()) {
                    continue;
                }
                path.push(from.id().clone());
                replacements.push(Box::pin(self.follow(from.id(), path)).await?);
                path.pop();
            }
        }
        Ok(dependencies::outcome(status, &replacements))
    }
}
