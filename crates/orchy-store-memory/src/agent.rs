use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::MemoryBackend;

impl AgentStore for MemoryBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        {
            let mut agents = self
                .agents
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            agents.insert(agent.id(), agent.clone());
        }

        let events = agent.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
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

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(agents
            .values()
            .find(|a| {
                a.org_id() == org && a.project() == project && a.alias() == Some(alias)
            })
            .cloned())
    }

    async fn list(&self, org: &OrganizationId) -> Result<Vec<Agent>> {
        let agents = self
            .agents
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(agents.values().filter(|a| a.org_id() == org).cloned().collect())
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
