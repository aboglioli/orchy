use std::sync::Arc;

use orchy_core::{Namespace, Role};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::rank_claimable::RankClaimable;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListReadyTasksCommand {
    pub namespace: Option<String>,
    pub role: Option<String>,
}

pub struct ListReadyTasks {
    ranking: Arc<RankClaimable>,
}

impl ListReadyTasks {
    pub fn new(ranking: Arc<RankClaimable>) -> Self {
        Self { ranking }
    }

    pub async fn execute(&self, cmd: ListReadyTasksCommand) -> ApplicationResult<Vec<TaskDto>> {
        let ranked = self
            .ranking
            .execute(
                cmd.namespace.as_deref().map(Namespace::new).transpose()?,
                cmd.role.as_deref().map(Role::new).transpose()?,
            )
            .await?;
        Ok(ranked.iter().map(TaskDto::from).collect())
    }
}
