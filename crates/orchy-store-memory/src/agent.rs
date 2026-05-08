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
        let conflict = self.state.agents.iter().find_map(|entry| {
            let a = entry.value();
            if a.org_id() == agent.org_id()
                && a.project() == agent.project()
                && a.alias() == agent.alias()
                && a.id() != agent.id()
            {
                Some(a.id().clone())
            } else {
                None
            }
        });
        if let Some(existing) = conflict {
            return Err(Error::conflict(format!(
                "alias '{}' already in use by agent {}",
                agent.alias(),
                existing
            )));
        }
        self.state.agents.insert(agent.id().clone(), agent.clone());

        let events = agent.drain_events();
        self.state.append_events(events).await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        Ok(self.state.agents.get(id).map(|r| r.clone()))
    }

    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        Ok(self.state.agents.iter().find_map(|entry| {
            let a = entry.value();
            if a.org_id() == org && a.project() == project && a.alias() == alias {
                Some(a.clone())
            } else {
                None
            }
        }))
    }

    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        let items: Vec<Agent> = self
            .state
            .agents
            .iter()
            .filter(|e| e.value().org_id() == org)
            .map(|e| e.value().clone())
            .collect();
        Ok(crate::apply_cursor_pagination(items, &page, |a| {
            a.id().to_string()
        }))
    }

    async fn find_by_ids(&self, ids: &[AgentId]) -> Result<Vec<Agent>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.state.agents.get(id).map(|r| r.clone()))
            .collect())
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        Ok(self
            .state
            .agents
            .iter()
            .filter(|e| e.value().is_timed_out(timeout_secs))
            .map(|e| e.value().clone())
            .collect())
    }
}
