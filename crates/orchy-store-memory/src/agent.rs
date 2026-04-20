use async_trait::async_trait;

use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::Result;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

use crate::MemoryBackend;

#[async_trait]
impl AgentStore for MemoryBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        {
            let mut agents = self.agents.write().await;
            agents.insert(agent.id().clone(), agent.clone());
        }

        let events = agent.drain_events();
        if !events.is_empty() {
            if let Err(e) = orchy_events::io::Writer::write_all(self, &events).await {
                tracing::error!("failed to persist events: {e}");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let agents = self.agents.read().await;
        Ok(agents.get(id).cloned())
    }

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let agents = self.agents.read().await;
        let items: Vec<Agent> = agents
            .values()
            .filter(|a| a.org_id() == org)
            .cloned()
            .collect();
        Ok(crate::apply_cursor_pagination(items, &page, |a| {
            a.id().to_string()
        }))
    }

    async fn find_by_ids(&self, ids: &[AgentId]) -> Result<Vec<Agent>> {
        let agents = self.agents.read().await;
        Ok(ids
            .iter()
            .filter_map(|id| agents.get(id))
            .cloned()
            .collect())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let agents = self.agents.read().await;

        Ok(agents
            .values()
            .filter(|a| a.is_timed_out(timeout_secs))
            .cloned()
            .collect())
    }
}
