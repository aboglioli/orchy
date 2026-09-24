use std::sync::Arc;

use chrono::Duration;
use orchy_core::{ActorId, LeaseStore, ResourceKey};
use serde::{Deserialize, Serialize};

use crate::dto::LeaseDto;
use crate::error::ApplicationResult;

const DEFAULT_TTL_SECS: i64 = 300;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ManageLeaseCommand {
    pub resource: String,
    pub actor: String,
    pub ttl_seconds: Option<i64>,
    pub action: LeaseAction,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeaseAction {
    #[default]
    Acquire,
    Release,
    Check,
}

pub struct ManageLease {
    leases: Arc<dyn LeaseStore>,
}

impl ManageLease {
    pub fn new(leases: Arc<dyn LeaseStore>) -> Self {
        Self { leases }
    }

    pub async fn execute(&self, cmd: ManageLeaseCommand) -> ApplicationResult<Option<LeaseDto>> {
        let key = ResourceKey::new(&cmd.resource)?;

        match cmd.action {
            LeaseAction::Acquire => {
                let actor: ActorId = cmd.actor.parse()?;
                let ttl = Duration::seconds(cmd.ttl_seconds.unwrap_or(DEFAULT_TTL_SECS));
                let lease = self.leases.acquire(&key, &actor, ttl).await?;
                Ok(Some(LeaseDto::from(&lease)))
            }
            LeaseAction::Release => {
                let actor: ActorId = cmd.actor.parse()?;
                self.leases.release(&key, &actor).await?;
                Ok(None)
            }
            LeaseAction::Check => Ok(self.leases.check(&key).await?.as_ref().map(LeaseDto::from)),
        }
    }
}
