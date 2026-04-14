use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_events::SerializedEvent;

use crate::MemoryBackend;

impl AgentStore for MemoryBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        let mut agents = self
            .agents
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        agents.insert(agent.id(), agent.clone());
        drop(agents);

        let events = agent.drain_events();
        if !events.is_empty() {
            let serialized: Vec<SerializedEvent> = events
                .iter()
                .filter_map(|e| SerializedEvent::from_event(e).ok())
                .collect();
            let mut store = self
                .events
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            store.extend(serialized);
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
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

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(agents
            .values()
            .filter(|a| a.is_timed_out(timeout_secs))
            .cloned()
            .collect())
    }
}
