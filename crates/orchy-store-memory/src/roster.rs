use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use orchy_core::{
    Actor, ActorId, ActorStore, Clock, DomainError, Lease, LeaseStore, ResourceKey, Result,
};

use crate::time::FixedClock;

const PRESENCE_TTL_SECS: i64 = 300;

#[derive(Default)]
pub struct MemoryActorStore(Mutex<BTreeMap<ActorId, Actor>>);

impl MemoryActorStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ActorStore for MemoryActorStore {
    async fn get(&self, id: &ActorId) -> Result<Option<Actor>> {
        Ok(self.0.lock().expect("actor mutex").get(id).cloned())
    }

    async fn roster(&self) -> Result<Vec<Actor>> {
        Ok(self
            .0
            .lock()
            .expect("actor mutex")
            .values()
            .cloned()
            .collect())
    }

    async fn save(&self, actor: &mut Actor) -> Result<()> {
        self.0
            .lock()
            .expect("actor mutex")
            .insert(actor.id().clone(), actor.clone());
        Ok(())
    }

    async fn present(&self, now: DateTime<Utc>) -> Result<Vec<ActorId>> {
        let cutoff = now - Duration::seconds(PRESENCE_TTL_SECS);
        Ok(self
            .0
            .lock()
            .expect("actor mutex")
            .values()
            .filter(|a| a.last_seen() >= cutoff)
            .map(|a| a.id().clone())
            .collect())
    }

    async fn touch(&self, id: &ActorId, now: DateTime<Utc>) -> Result<()> {
        let mut actors = self.0.lock().expect("actor mutex");
        let actor = actors
            .get_mut(id)
            .ok_or_else(|| DomainError::not_found("actor", id))?;
        actor.seen_at(now);
        Ok(())
    }
}

pub struct MemoryLeaseStore {
    leases: Mutex<BTreeMap<ResourceKey, Lease>>,
    clock: Arc<FixedClock>,
    generation: Mutex<u64>,
}

impl MemoryLeaseStore {
    pub fn new(clock: Arc<FixedClock>) -> Self {
        Self {
            leases: Mutex::new(BTreeMap::new()),
            clock,
            generation: Mutex::new(0),
        }
    }
}

#[async_trait]
impl LeaseStore for MemoryLeaseStore {
    async fn acquire(&self, key: &ResourceKey, by: &ActorId, ttl: Duration) -> Result<Lease> {
        let now = self.clock.now();
        let mut leases = self.leases.lock().expect("lease mutex");

        if let Some(existing) = leases.get(key)
            && !existing.is_expired_at(now)
            && existing.holder() != by
        {
            return Err(DomainError::conflict(format!(
                "`{key}` is held by {} until {}",
                existing.holder(),
                existing.expires_at()
            )));
        }

        let mut generation = self.generation.lock().expect("generation mutex");
        *generation += 1;
        let lease = Lease::create(key.clone(), by.clone(), ttl, now, *generation);
        leases.insert(key.clone(), lease.clone());
        Ok(lease)
    }

    async fn release(&self, key: &ResourceKey, by: &ActorId) -> Result<()> {
        let mut leases = self.leases.lock().expect("lease mutex");
        match leases.get(key) {
            Some(lease) if lease.holder() == by => {
                leases.remove(key);
                Ok(())
            }
            Some(lease) => Err(DomainError::forbidden(format!(
                "`{key}` is held by {}, not {by}",
                lease.holder()
            ))),
            None => Ok(()),
        }
    }

    async fn check(&self, key: &ResourceKey) -> Result<Option<Lease>> {
        let now = self.clock.now();
        Ok(self
            .leases
            .lock()
            .expect("lease mutex")
            .get(key)
            .filter(|l| !l.is_expired_at(now))
            .cloned())
    }
}
