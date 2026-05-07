use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::graph::EdgeStore;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;

pub struct FailTaskCommand {
    pub task_id: String,
    pub org_id: String,
    pub reason: Option<String>,
}

pub struct FailTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl FailTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: FailTaskCommand) -> ApplicationResult<TaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        if task.org_id() != &org_id {
            return Err(Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            }
            .into());
        }

        task.fail(cmd.reason)?;
        self.tasks.save(&mut task).await?;

        if let Err(e) = crate::complete_task::try_auto_complete_parent(
            &self.tasks,
            &self.edges,
            &org_id,
            &task_id,
        )
        .await
        {
            tracing::warn!("failed to check parent auto-complete for {task_id}: {e}");
        }

        Ok(TaskDto::from(&task))
    }
}
