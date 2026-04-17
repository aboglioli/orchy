use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

pub struct ClaimTaskCommand {
    pub task_id: String,
    pub agent_id: String,
    pub start: Option<bool>,
}

pub struct ClaimTask {
    tasks: Arc<dyn TaskStore>,
}

impl ClaimTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ClaimTaskCommand) -> Result<Task> {
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

        for dep_id in task.depends_on() {
            let dep = self
                .tasks
                .find_by_id(dep_id)
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
        Ok(task)
    }
}
