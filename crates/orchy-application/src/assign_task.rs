use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct AssignTaskCommand {
    pub task_id: String,
    pub agent_id: String,
}

pub struct AssignTask {
    tasks: Arc<dyn TaskStore>,
}

impl AssignTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: AssignTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.assign(agent_id)?;
        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
