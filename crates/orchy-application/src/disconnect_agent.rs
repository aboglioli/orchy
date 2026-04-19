use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::resource_lock::LockStore;
use orchy_core::task::{TaskFilter, TaskStore};

pub struct DisconnectAgentCommand {
    pub agent_id: String,
}

pub struct DisconnectAgent {
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
    locks: Arc<dyn LockStore>,
}

impl DisconnectAgent {
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

    pub async fn execute(&self, cmd: DisconnectAgentCommand) -> Result<()> {
        let id = AgentId::from_str(&cmd.agent_id)?;

        let mut agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;

        self.release_tasks(&id).await?;
        self.release_locks(&id, agent.org_id()).await?;

        agent.disconnect()?;
        self.agents.save(&mut agent).await
    }

    async fn release_tasks(&self, agent_id: &AgentId) -> Result<()> {
        let tasks = self
            .tasks
            .list(
                TaskFilter {
                    assigned_to: Some(agent_id.clone()),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items;

        for mut task in tasks {
            task.release()?;
            self.tasks.save(&mut task).await?;
        }

        Ok(())
    }

    async fn release_locks(&self, agent_id: &AgentId, org: &OrganizationId) -> Result<()> {
        let locks = self.locks.find_by_holder(agent_id, org).await?;

        for mut lock in locks {
            lock.mark_released()?;
            self.locks.save(&mut lock).await?;
            self.locks
                .delete(lock.org_id(), lock.project(), lock.namespace(), lock.name())
                .await?;
        }

        Ok(())
    }
}
