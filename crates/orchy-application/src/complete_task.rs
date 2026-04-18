use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct CompleteTaskCommand {
    pub task_id: String,
    pub summary: Option<String>,
}

pub struct CompleteTask {
    tasks: Arc<dyn TaskStore>,
}

impl CompleteTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: CompleteTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.complete(cmd.summary)?;
        self.tasks.save(&mut task).await?;

        if let Err(e) = self.try_auto_complete_parent(&task_id).await {
            tracing::warn!("failed to check parent auto-complete for {task_id}: {e}");
        }

        Ok(TaskResponse::from(&task))
    }

    async fn try_auto_complete_parent(&self, task_id: &TaskId) -> Result<()> {
        let Some(task) = self.tasks.find_by_id(task_id).await? else {
            return Ok(());
        };
        let Some(parent_id) = task.parent_id() else {
            return Ok(());
        };
        let Some(mut parent) = self.tasks.find_by_id(&parent_id).await? else {
            return Ok(());
        };
        let children = self
            .tasks
            .list(
                TaskFilter {
                    parent_id: Some(parent.id()),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items;

        let all_done = children
            .iter()
            .all(|c| matches!(c.status(), TaskStatus::Completed | TaskStatus::Cancelled));

        if all_done {
            let _ = parent.auto_complete("all subtasks completed".to_string());
            if let Err(e) = self.tasks.save(&mut parent).await {
                tracing::warn!("failed to auto-complete parent {}: {e}", parent_id);
            }
        }

        Ok(())
    }
}
