use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::Result;
use orchy_core::pagination::PageParams;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

use crate::dto::AgentDto;

pub struct CheckTimedOutAgentsResult {
    pub agents: Vec<AgentDto>,
    pub locks_released: u64,
    pub tasks_released: u64,
}

pub struct CheckTimedOutAgents {
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
    locks: Arc<dyn LockStore>,
}

impl CheckTimedOutAgents {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        tasks: Arc<dyn TaskStore>,
        locks: Arc<dyn LockStore>,
    ) -> Self {
        Self {
            agents,
            tasks,
            locks,
        }
    }

    pub async fn execute(&self, timeout_secs: u64) -> Result<CheckTimedOutAgentsResult> {
        let agents = self.agents.find_timed_out(timeout_secs).await?;

        let mut locks_released = 0u64;
        let mut tasks_released = 0u64;

        for agent in &agents {
            let org_id = agent.org_id().clone();
            locks_released += self.locks.release_for_agent(agent.id(), &org_id).await?;

            for status in [TaskStatus::Claimed, TaskStatus::InProgress] {
                let filter = TaskFilter {
                    org_id: Some(org_id.clone()),
                    assigned_to: Some(agent.id().clone()),
                    status: Some(status),
                    ..Default::default()
                };
                let mut page = self.tasks.list(filter, PageParams::unbounded()).await?;
                for mut task in page.items.drain(..) {
                    task.release()?;
                    self.tasks.save(&mut task).await?;
                    tasks_released += 1;
                }
            }
        }

        Ok(CheckTimedOutAgentsResult {
            agents: agents.iter().map(AgentDto::from).collect(),
            locks_released,
            tasks_released,
        })
    }
}
