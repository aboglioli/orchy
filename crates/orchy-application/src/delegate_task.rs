use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct DelegateTaskCommand {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub created_by: Option<String>,
}

pub struct DelegateTask {
    tasks: Arc<dyn TaskStore>,
}

impl DelegateTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: DelegateTaskCommand) -> Result<TaskResponse> {
        let parent_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let created_by = cmd
            .created_by
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let parent = self
            .tasks
            .find_by_id(&parent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {parent_id}")))?;

        let priority = cmd
            .priority
            .map(|p| p.parse::<Priority>())
            .transpose()
            .map_err(Error::InvalidInput)?
            .unwrap_or_default();

        let mut subtask = Task::new(
            parent.org_id().clone(),
            parent.project().clone(),
            parent.namespace().clone(),
            Some(parent_id),
            cmd.title,
            cmd.description,
            priority,
            cmd.assigned_roles.unwrap_or_default(),
            vec![],
            created_by,
            false,
        )?;

        self.tasks.save(&mut subtask).await?;
        Ok(TaskResponse::from(&subtask))
    }
}
