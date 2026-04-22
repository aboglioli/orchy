use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct CancelTaskCommand {
    pub task_id: String,
    pub org_id: String,
    pub reason: Option<String>,
}

pub struct CancelTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl CancelTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: CancelTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.cancel(cmd.reason)?;
        self.tasks.save(&mut task).await?;

        if let Err(e) = self.try_auto_complete_parent(&org_id, &task_id).await {
            tracing::warn!("failed to check parent auto-complete for {task_id}: {e}");
        }

        Ok(TaskResponse::from(&task))
    }

    async fn try_auto_complete_parent(&self, org: &OrganizationId, task_id: &TaskId) -> Result<()> {
        let parent_edges = self
            .edges
            .find_to(
                org,
                &ResourceKind::Task,
                &task_id.to_string(),
                &[RelationType::Spawns],
                None,
            )
            .await?;
        let Some(parent_edge) = parent_edges.first() else {
            return Ok(());
        };
        let parent_id: TaskId = parent_edge
            .from_id()
            .parse()
            .map_err(|_| Error::InvalidInput("invalid parent task id".to_string()))?;

        let Some(mut parent) = self.tasks.find_by_id(&parent_id).await? else {
            return Ok(());
        };
        if parent.status() == TaskStatus::Completed {
            return Ok(());
        }

        let sibling_edges = self
            .edges
            .find_from(
                org,
                &ResourceKind::Task,
                &parent_id.to_string(),
                &[RelationType::Spawns],
                None,
            )
            .await?;
        let sibling_ids: Vec<TaskId> = sibling_edges
            .iter()
            .filter_map(|e| e.to_id().parse::<TaskId>().ok())
            .collect();
        let siblings = self.tasks.find_by_ids(&sibling_ids).await?;

        if Task::all_children_completed(&siblings) {
            if let Err(e) = parent.auto_complete("all subtasks completed".to_string()) {
                tracing::warn!("auto_complete rejected for parent {parent_id}: {e}");
            } else if let Err(e) = self.tasks.save(&mut parent).await {
                tracing::warn!("failed to save auto-completed parent {parent_id}: {e}");
            }
        }

        Ok(())
    }
}
