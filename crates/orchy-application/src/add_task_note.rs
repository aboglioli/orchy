use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskId, TaskStore};

pub struct AddTaskNoteCommand {
    pub task_id: String,
    pub body: String,
    pub author: Option<String>,
}

pub struct AddTaskNote {
    tasks: Arc<dyn TaskStore>,
}

impl AddTaskNote {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: AddTaskNoteCommand) -> Result<Task> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let author = cmd
            .author
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.add_note(author, cmd.body)?;
        self.tasks.save(&mut task).await?;
        Ok(task)
    }
}
