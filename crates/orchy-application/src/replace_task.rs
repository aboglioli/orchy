use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::dto::TaskResponse;
use crate::split_task::SubtaskInput;

pub struct ReplaceTaskCommand {
    pub task_id: String,
    pub reason: Option<String>,
    pub replacements: Vec<SubtaskInput>,
    pub created_by: Option<String>,
}

pub struct ReplaceTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl ReplaceTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(
        &self,
        cmd: ReplaceTaskCommand,
    ) -> Result<(TaskResponse, Vec<TaskResponse>)> {
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

        let org_id = original.org_id().clone();

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
            let is_blocked = !depends_on.is_empty();

            let mut task = Task::new(
                org_id.clone(),
                original.project().clone(),
                original.namespace().clone(),
                original.parent_id(),
                input.title,
                input.description,
                input.acceptance_criteria,
                priority,
                input.assigned_roles.unwrap_or_default(),
                depends_on,
                created_by.clone(),
                is_blocked,
            )?;
            self.tasks.save(&mut task).await?;

            for dep_id in task.depends_on() {
                let already_exists = self
                    .edges
                    .exists_by_pair(
                        &org_id,
                        &ResourceKind::Task,
                        &task.id().to_string(),
                        &ResourceKind::Task,
                        &dep_id.to_string(),
                        &RelationType::DependsOn,
                    )
                    .await?;
                if !already_exists {
                    let dep_edge = Edge::new(
                        org_id.clone(),
                        ResourceKind::Task,
                        task.id().to_string(),
                        ResourceKind::Task,
                        dep_id.to_string(),
                        RelationType::DependsOn,
                        None,
                        created_by.clone(),
                    );
                    self.edges.save(&dep_edge).await?;
                }
            }

            let edge = Edge::new(
                org_id.clone(),
                ResourceKind::Task,
                task.id().to_string(),
                ResourceKind::Task,
                task_id.to_string(),
                RelationType::Supersedes,
                None,
                created_by.clone(),
            )
            .with_source(ResourceKind::Task, task_id.to_string());
            self.edges.save(&edge).await?;

            new_tasks.push(task);
        }

        Ok((
            TaskResponse::from(&original),
            new_tasks.iter().map(TaskResponse::from).collect(),
        ))
    }
}
