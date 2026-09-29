use std::sync::Arc;

use orchy_core::{Namespace, Role};
use serde::{Deserialize, Serialize};

use crate::claim_task::{ClaimTask, ClaimTaskCommand};
use crate::dto::TaskDto;
use crate::error::{ApplicationError, ApplicationResult};
use crate::rank_claimable::RankClaimable;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NextTaskCommand {
    pub actor: String,
    pub role: Option<String>,
    pub namespace: Option<String>,
    pub claim: bool,
}

pub struct NextTask {
    ranking: Arc<RankClaimable>,
    claim: Arc<ClaimTask>,
}

impl NextTask {
    pub fn new(ranking: Arc<RankClaimable>, claim: Arc<ClaimTask>) -> Self {
        Self { ranking, claim }
    }

    pub async fn execute(&self, cmd: NextTaskCommand) -> ApplicationResult<Option<TaskDto>> {
        let candidates = self
            .ranking
            .execute(
                cmd.namespace.as_deref().map(Namespace::new).transpose()?,
                cmd.role.as_deref().map(Role::new).transpose()?,
            )
            .await?;

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
