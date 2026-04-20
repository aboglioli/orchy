use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::dto::TaskResponse;

pub struct DelegateTaskCommand {
    pub task_id: String,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub created_by: Option<String>,
}

pub struct DelegateTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl DelegateTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: DelegateTaskCommand) -> Result<TaskResponse> {
        let parent_id = cmd.task_id.parse::<TaskId>()?;

        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

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
            cmd.acceptance_criteria,
            priority,
            cmd.assigned_roles.unwrap_or_default(),
            vec![],
            created_by.clone(),
            false,
        )?;

        self.tasks.save(&mut subtask).await?;
        let mut edge = match Edge::new(
            parent.org_id().clone(),
            ResourceKind::Task,
            parent_id.to_string(),
            ResourceKind::Task,
            subtask.id().to_string(),
            RelationType::Spawns,
            created_by,
        ) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("failed to create edge: {e}");
                return Ok(TaskResponse::from(&subtask));
            }
        }
        .with_source(ResourceKind::Task, parent_id.to_string());
        if let Err(e) = self.edges.save(&mut edge).await {
            tracing::warn!(
                "failed to create spawns edge for delegated task {}: {e}",
                subtask.id()
            );
        }
        Ok(TaskResponse::from(&subtask))
    }
}
