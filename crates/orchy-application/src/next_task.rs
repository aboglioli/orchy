use std::sync::Arc;

use orchy_core::{Namespace, PageRequest, Role, Task, TaskQuery, TaskStatus, TaskStore};
use serde::{Deserialize, Serialize};

use crate::claim_task::{ClaimTask, ClaimTaskCommand};
use crate::dto::TaskDto;
use crate::error::{ApplicationError, ApplicationResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NextTaskCommand {
    pub actor: String,
    pub role: Option<String>,
    pub namespace: Option<String>,
    pub claim: bool,
}

pub struct NextTask {
    tasks: Arc<dyn TaskStore>,
    claim: Arc<ClaimTask>,
}

impl NextTask {
    pub fn new(tasks: Arc<dyn TaskStore>, claim: Arc<ClaimTask>) -> Self {
        Self { tasks, claim }
    }

    pub async fn execute(&self, cmd: NextTaskCommand) -> ApplicationResult<Option<TaskDto>> {
        let query = TaskQuery {
            status: Some(vec![TaskStatus::Pending]),
            namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
            role: cmd.role.as_deref().map(Role::new).transpose()?,
            ..Default::default()
        };
        let mut candidates = self
            .tasks
            .find(&query, PageRequest::new(0, 1000))
            .await?
            .items;

        candidates.retain(|t| t.depends_on().is_empty());
        candidates.sort_by(|a: &Task, b: &Task| {
            b.priority()
                .cmp(&a.priority())
                .then_with(|| a.created_at().cmp(&b.created_at()))
                .then_with(|| a.id().cmp(b.id()))
        });

        let Some(first) = candidates.first() else {
            return Ok(None);
        };
        if !cmd.claim {
            return Ok(Some(TaskDto::from(first)));
        }

        // Another agent may take a task between ranking it and claiming it, which is ordinary
        // under several workers rather than an error: walk down the ranking until one sticks.
        for candidate in &candidates {
            match self
                .claim
                .execute(ClaimTaskCommand {
                    task_id: candidate.id().to_string(),
                    actor: cmd.actor.clone(),
                    ttl_seconds: None,
                    start: false,
                })
                .await
            {
                Ok(claimed) => return Ok(Some(claimed)),
                Err(e) if is_contention(&e) => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
}

fn is_contention(error: &ApplicationError) -> bool {
    matches!(
        error,
        ApplicationError::Domain(orchy_core::DomainError::Conflict(_))
    )
}
