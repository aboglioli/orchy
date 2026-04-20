use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::edge::{EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct ClaimTaskCommand {
    pub task_id: String,
    pub agent_id: String,
    pub org_id: String,
    pub start: Option<bool>,
}

pub struct ClaimTask {
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl ClaimTask {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
    ) -> Self {
        Self {
            agents,
            tasks,
            edges,
        }
    }

    pub async fn execute(&self, cmd: ClaimTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let agent_id = AgentId::from_str(&cmd.agent_id)?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        self.agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let dep_edges = self
            .edges
            .find_from(
                &org_id,
                &ResourceKind::Task,
                &task_id.to_string(),
                &[RelationType::DependsOn],
                None,
            )
            .await?;

        for edge in &dep_edges {
            let dep_id: TaskId = match edge.to_id().parse() {
                Ok(id) => id,
                Err(_) => continue,
            };
            let dep = self
                .tasks
                .find_by_id(&dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Err(Error::DependencyNotMet(task_id.to_string()));
            }
        }

        task.claim(agent_id.clone())?;

        if cmd.start.unwrap_or(false) {
            task.start(&agent_id)?;
        }

        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
