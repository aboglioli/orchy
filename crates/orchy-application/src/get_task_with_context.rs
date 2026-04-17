use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskId, TaskStore, TaskWithContext};

pub struct GetTaskWithContext {
    tasks: Arc<dyn TaskStore>,
}

impl GetTaskWithContext {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, task_id: &str) -> Result<TaskWithContext> {
        let id = task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        self.get_with_context(&id).await
    }

    pub async fn get_with_context(&self, id: &TaskId) -> Result<TaskWithContext> {
        let task = self
            .tasks
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        let mut ancestors = Vec::new();
        let mut current_parent_id = task.parent_id();
        while let Some(pid) = current_parent_id {
            match self.tasks.find_by_id(&pid).await? {
                Some(parent) => {
                    current_parent_id = parent.parent_id();
                    ancestors.push(parent);
                }
                None => break,
            }
        }

        let children = self
            .tasks
            .list(
                TaskFilter {
                    parent_id: Some(*id),
                    ..Default::default()
                },
                PageParams::unbounded(),
            )
            .await?
            .items;

        Ok(TaskWithContext {
            task,
            ancestors,
            children,
        })
    }
}
