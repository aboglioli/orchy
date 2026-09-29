use std::sync::Arc;

use chrono::Duration;
use orchy_core::{
    ActorId, Clock, DomainError, EventLog, Lease, LeaseChange, LeaseChanged, LeaseStore,
    ResourceKey,
};
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
    log: Arc<dyn EventLog>,
    clock: Arc<dyn Clock>,
}

impl ManageLease {
    pub fn new(leases: Arc<dyn LeaseStore>, log: Arc<dyn EventLog>, clock: Arc<dyn Clock>) -> Self {
        Self { leases, log, clock }
    }

    async fn record(
        &self,
        change: LeaseChange,
        resource: &ResourceKey,
        holder: &ActorId,
        lease: Option<&Lease>,
    ) -> ApplicationResult<()> {
        let event = LeaseChanged {
            change,
            resource: resource.clone(),
            holder: holder.clone(),
            expires_at: lease.map(Lease::expires_at),
            at: self.clock.now(),
        };
        Ok(self.log.append(&[Box::new(event)]).await?)
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
                self.record(LeaseChange::Acquired, &key, &actor, Some(&lease))
                    .await?;
                Ok(Some(LeaseDto::from(&lease)))
            }
            LeaseAction::Renew => {
                let actor: ActorId = cmd.actor.parse()?;
                let lease = self.leases.renew(&key, &actor, ttl).await?;
                self.record(LeaseChange::Renewed, &key, &actor, Some(&lease))
                    .await?;
                Ok(Some(LeaseDto::from(&lease)))
            }
            LeaseAction::Release => {
                let actor: ActorId = cmd.actor.parse()?;
                self.leases.release(&key, &actor).await?;
                self.record(LeaseChange::Released, &key, &actor, None)
                    .await?;
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
