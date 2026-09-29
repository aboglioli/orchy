use std::sync::Arc;

use orchy_core::task::dependencies::Outcome;
use orchy_core::task::ranking;
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

        let mut ready = Vec::new();
        for task in pending {
            if self.dependencies.outcome(&task).await? == Outcome::Satisfied {
                ready.push(task);
            }
        }
        Ok(ranking::claimable(ready, |_| true))
    }
}
