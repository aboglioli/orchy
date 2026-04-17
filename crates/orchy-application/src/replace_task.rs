use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::split_task::SubtaskInput;

pub struct ReplaceTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
    pub replacements: Vec<SubtaskInput>,
    pub created_by: Option<String>,
}

pub struct ReplaceTask {
    tasks: Arc<dyn TaskStore>,
}

impl ReplaceTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ReplaceTaskCommand) -> Result<(Task, Vec<Task>)> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let created_by = cmd
            .created_by
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut original = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let cancel_reason = cmd
            .reason
            .unwrap_or_else(|| "replaced by new tasks".to_string());
        original.cancel(Some(cancel_reason))?;
        self.tasks.save(&mut original).await?;

        let mut new_tasks = Vec::with_capacity(cmd.replacements.len());
        for input in cmd.replacements {
            let priority = input
                .priority
                .map(|p| p.parse::<Priority>())
                .transpose()
                .map_err(Error::InvalidInput)?
                .unwrap_or_default();

            let depends_on = input
                .depends_on
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.parse::<TaskId>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::InvalidInput(e.to_string()))?;

            let mut task = Task::new(
                original.org_id().clone(),
                original.project().clone(),
                original.namespace().clone(),
                original.parent_id(),
                input.title,
                input.description,
                priority,
                input.assigned_roles.unwrap_or_default(),
                depends_on,
                created_by.clone(),
                false,
            )?;
            self.tasks.save(&mut task).await?;
            new_tasks.push(task);
        }

        Ok((original, new_tasks))
    }
}
