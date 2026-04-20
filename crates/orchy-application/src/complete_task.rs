use std::sync::Arc;

use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::graph::neighborhood::LinkParam;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskResponse;

pub struct CompleteTaskCommand {
    pub task_id: String,
    pub org_id: String,
    pub summary: Option<String>,
    pub links: Vec<LinkParam>,
}

pub struct CompleteTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl CompleteTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: CompleteTaskCommand) -> Result<TaskResponse> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        task.complete(cmd.summary)?;
        self.tasks.save(&mut task).await?;

        for link in cmd.links {
            let to_kind = link
                .to_kind
                .parse::<ResourceKind>()
                .map_err(Error::InvalidInput)?;
            let rel_type = link
                .rel_type
                .parse::<RelationType>()
                .map_err(Error::InvalidInput)?;
            let exists = self
                .edges
                .exists_by_pair(
                    &org_id,
                    &ResourceKind::Task,
                    &task_id.to_string(),
                    &to_kind,
                    &link.to_id,
                    &rel_type,
                )
                .await?;
            if !exists {
                let mut edge = Edge::new(
                    org_id.clone(),
                    ResourceKind::Task,
                    task_id.to_string(),
                    to_kind,
                    link.to_id,
                    rel_type,
                    None,
                )?;
                self.edges.save(&mut edge).await?;
            }
        }

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
            parent.auto_complete("all subtasks completed".to_string())?;
            if let Err(e) = self.tasks.save(&mut parent).await {
                tracing::warn!("failed to auto-complete parent {parent_id}: {e}");
            }
        }
        Ok(())
    }
}
