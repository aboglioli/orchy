use std::sync::Arc;

use crate::entities::{Agent, RegisterAgent};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::AgentId;

pub struct AgentService {
    store: Arc<Store>,
}

impl AgentService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        self.store.register(registration).await
    }

    pub async fn get(&self, id: &AgentId) -> Result<Agent> {
        self.store
            .get_agent(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }

    pub async fn list(&self) -> Result<Vec<Agent>> {
        self.store.list_agents().await
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        self.store.heartbeat(id).await
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        self.store.disconnect(id).await
    }

    pub async fn disconnect_timed_out(&self, timeout_secs: u64) -> Result<Vec<AgentId>> {
        let agents = self.store.find_timed_out(timeout_secs).await?;
        let ids: Vec<AgentId> = agents.iter().map(|a| a.id).collect();
        for id in &ids {
            self.store.disconnect(id).await?;
        }
        Ok(ids)
    }
}
