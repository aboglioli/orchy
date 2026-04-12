use chrono::Utc;

use orchy_core::entities::{Agent, RegisterAgent};
use orchy_core::error::{Error, Result};
use orchy_core::store::AgentStore;
use orchy_core::value_objects::{AgentId, AgentStatus};

use crate::MemoryBackend;

impl AgentStore for MemoryBackend {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        let now = Utc::now();
        let agent = Agent {
            id: AgentId::new(),
            namespace: registration.namespace.clone(),
            roles: registration.roles,
            description: registration.description,
            status: AgentStatus::Online,
            last_heartbeat: now,
            connected_at: now,
            metadata: registration.metadata,
        };

        let mut agents = self
            .agents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        agents.insert(agent.id, agent.clone());
        Ok(agent)
    }

    async fn get(&self, id: &AgentId) -> Result<Option<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(agents.get(id).cloned())
    }

    async fn list(&self) -> Result<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(agents.values().cloned().collect())
    }

    async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let agent = agents
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.last_heartbeat = Utc::now();
        if agent.status == AgentStatus::Disconnected {
            agent.status = AgentStatus::Online;
        }
        Ok(())
    }

    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let agent = agents
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.status = status;
        Ok(())
    }

    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let agent = agents
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
        agent.status = AgentStatus::Disconnected;
        Ok(())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let now = Utc::now();
        let timeout = chrono::Duration::seconds(timeout_secs as i64);

        Ok(agents
            .values()
            .filter(|a| a.status != AgentStatus::Disconnected && (now - a.last_heartbeat) > timeout)
            .cloned()
            .collect())
    }
}
