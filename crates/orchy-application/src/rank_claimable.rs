use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::task::dependencies::Outcome;
use orchy_core::task::{ranking, rollup};
use orchy_core::{Namespace, Role, Task, TaskQuery, TaskStatus, TaskStore};

use crate::assess_dependencies::AssessDependencies;
use crate::error::ApplicationResult;

pub struct RankClaimable {
    tasks: Arc<dyn TaskStore>,
    dependencies: Arc<AssessDependencies>,
}

impl RankClaimable {
    pub fn new(tasks: Arc<dyn TaskStore>, dependencies: Arc<AssessDependencies>) -> Self {
        Self {
            tasks,
            dependencies,
        }
    }

    pub async fn execute(
        &self,
        namespace: Option<Namespace>,
        role: Option<Role>,
    ) -> ApplicationResult<Vec<Task>> {
        let pending = self
            .tasks
            .matching(&TaskQuery {
                status: Some(vec![TaskStatus::Pending]),
                namespace,
                role,
                ..Default::default()
            })
            .await?;

        let everything = self.tasks.matching(&TaskQuery::default()).await?;
        let mut waiting: HashMap<_, usize> = HashMap::new();
        for task in everything.iter().filter(|t| !t.status().is_terminal()) {
            for dependency in task.depends_on() {
                *waiting.entry(dependency.clone()).or_default() += 1;
            }
        }
        let children_of = |parent: &Task| -> Vec<TaskStatus> {
            everything
                .iter()
                .filter(|t| t.parent() == Some(parent.id()))
                .map(Task::status)
                .collect()
        };

        let mut ready = Vec::new();
        for task in pending {
            if rollup::ensure_claimable(&children_of(&task)).is_err() {
                continue;
            }
            if self.dependencies.outcome(&task).await? == Outcome::Satisfied {
                ready.push(task);
            }
        }
        Ok(ranking::claimable(
            ready,
            |_| true,
            |t| waiting.get(t.id()).copied().unwrap_or_default(),
        ))
    }
}
