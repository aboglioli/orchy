use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::resource_ref::{ResourceKind, ResourceRef};
use orchy_core::task::{Priority, TaskId, TaskStore};

use crate::dto::TaskResponse;
use crate::post_task::ResourceRefInput;

pub struct UpdateTaskCommand {
    pub task_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub priority: Option<String>,
    pub add_refs: Option<Vec<ResourceRefInput>>,
    pub remove_refs: Option<Vec<ResourceRefInput>>,
}

pub struct UpdateTask {
    tasks: Arc<dyn TaskStore>,
}

impl UpdateTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: UpdateTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd
            .task_id
            .parse::<TaskId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let priority = cmd
            .priority
            .map(|p| p.parse::<Priority>())
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.update_details(cmd.title, cmd.description, priority)?;

        if let Some(refs) = cmd.add_refs {
            for r in refs {
                let kind = r
                    .kind
                    .parse::<ResourceKind>()
                    .map_err(Error::InvalidInput)?;
                let mut rr = ResourceRef::new(kind, r.id);
                if let Some(d) = r.display {
                    rr = rr.with_display(d);
                }
                task.add_ref(rr);
            }
        }

        if let Some(refs) = cmd.remove_refs {
            for r in refs {
                let kind = r
                    .kind
                    .parse::<ResourceKind>()
                    .map_err(Error::InvalidInput)?;
                task.remove_ref(&ResourceRef::new(kind, r.id));
            }
        }

        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
