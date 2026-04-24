use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

use crate::MemoryState;

pub struct MemoryAgentStore {
    state: Arc<MemoryState>,
}

impl MemoryAgentStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AgentStore for MemoryAgentStore {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        {
            let mut agents = self.state.agents.write().await;
            if let Some(existing) = agents.values().find(|a| {
                a.org_id() == agent.org_id()
                    && a.project() == agent.project()
                    && a.alias() == agent.alias()
                    && a.id() != agent.id()
            }) {
                return Err(Error::Conflict(format!(
                    "alias '{}' already in use by agent {}",
                    agent.alias(),
                    existing.id()
                )));
            }
            agents.insert(agent.id().clone(), agent.clone());
        }

        let events = agent.drain_events();
        if !events.is_empty() {
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| orchy_core::error::Error::Store(e.to_string()))?;
                self.state.events.write().await.push(serialized);
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        let agents = self.state.agents.read().await;
        Ok(agents.get(id).cloned())
    }

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        let agents = self.state.agents.read().await;
        Ok(agents
            .values()
            .find(|a| a.org_id() == org && a.project() == project && a.alias() == alias)
            .cloned())
    }

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let agents = self.state.agents.read().await;
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
        let agents = self.state.agents.read().await;
        Ok(ids
            .iter()
            .filter_map(|id| agents.get(id))
            .cloned()
            .collect())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        let agents = self.state.agents.read().await;

        Ok(agents
            .values()
            .filter(|a| a.is_timed_out(timeout_secs))
            .cloned()
            .collect())
    }
}
