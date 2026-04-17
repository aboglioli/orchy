use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, WatcherStore};

pub struct UnwatchTaskCommand {
    pub task_id: String,
    pub agent_id: String,
}

pub struct UnwatchTask {
    watchers: Arc<dyn WatcherStore>,
}

impl UnwatchTask {
    pub fn new(watchers: Arc<dyn WatcherStore>) -> Self {
        Self { watchers }
    }

    pub async fn execute(&self, cmd: UnwatchTaskCommand) -> Result<()> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;

        self.watchers.delete(&task_id, &agent_id).await
    }
}
