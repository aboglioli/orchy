use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct StartTaskCommand {
    pub task_id: String,
    pub agent_id: String,
}

pub struct StartTask {
    agents: Arc<dyn AgentStore>,
    tasks: Arc<dyn TaskStore>,
}

impl StartTask {
    pub fn new(agents: Arc<dyn AgentStore>, tasks: Arc<dyn TaskStore>) -> Self {
        Self { agents, tasks }
    }

    pub async fn execute(&self, cmd: StartTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;

        self.agents
            .find_by_id(&agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {agent_id}")))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.start(&agent_id)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
