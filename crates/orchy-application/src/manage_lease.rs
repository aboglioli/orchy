use std::sync::Arc;

use chrono::Duration;
use orchy_core::{ActorId, DomainError, LeaseStore, ResourceKey};
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
    Renew,
    Release,
    Check,
    Held,
}

pub struct ManageLease {
    leases: Arc<dyn LeaseStore>,
}

impl ManageLease {
    pub fn new(leases: Arc<dyn LeaseStore>) -> Self {
        Self { leases }
    }

    pub async fn execute(&self, cmd: ManageLeaseCommand) -> ApplicationResult<Option<LeaseDto>> {
        if cmd.action == LeaseAction::Held {
            return Ok(None);
        }
        let key = ResourceKey::new(&cmd.resource)?;
        let seconds = cmd.ttl_seconds.unwrap_or(DEFAULT_TTL_SECS);
        if seconds <= 0 {
            return Err(DomainError::validation(format!(
                "ttl must be a positive number of seconds, not {seconds}"
            ))
            .into());
        }
        let ttl = Duration::seconds(seconds);

        match cmd.action {
            LeaseAction::Acquire => {
                let actor: ActorId = cmd.actor.parse()?;
                let lease = self.leases.acquire(&key, &actor, ttl).await?;
                Ok(Some(LeaseDto::from(&lease)))
            }
            LeaseAction::Renew => {
                let actor: ActorId = cmd.actor.parse()?;
                let lease = self.leases.renew(&key, &actor, ttl).await?;
                Ok(Some(LeaseDto::from(&lease)))
            }
            LeaseAction::Release => {
                let actor: ActorId = cmd.actor.parse()?;
                self.leases.release(&key, &actor).await?;
                Ok(None)
            }
            LeaseAction::Check => Ok(self.leases.check(&key).await?.as_ref().map(LeaseDto::from)),
            LeaseAction::Held => unreachable!("answered before a key is required"),
        }
    }

    pub async fn held(&self) -> ApplicationResult<Vec<LeaseDto>> {
        Ok(self
            .leases
            .held()
            .await?
            .iter()
            .map(LeaseDto::from)
            .collect())
    }
}
