use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;

pub struct SubtaskInput {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
}

pub struct SplitTaskCommand {
    pub task_id: String,
    pub subtasks: Vec<SubtaskInput>,
    pub created_by: Option<String>,
}

pub struct SplitTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl SplitTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: SplitTaskCommand) -> Result<(TaskDto, Vec<TaskDto>)> {
        let parent_id = cmd.task_id.parse::<TaskId>()?;

        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

        let mut parent = self
            .tasks
            .find_by_id(&parent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {parent_id}")))?;

        if matches!(
            parent.status(),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot split task {} with status {}",
                parent_id,
                parent.status()
            )));
        }

        struct SubtaskSpec {
            title: String,
            description: String,
            acceptance_criteria: Option<String>,
            priority: Priority,
            assigned_roles: Vec<String>,
            depends_on_ids: Vec<TaskId>,
        }

        let mut specs = Vec::with_capacity(cmd.subtasks.len());
        for input in cmd.subtasks {
            let priority = input
                .priority
                .map(|p| p.parse::<Priority>())
                .transpose()
                .map_err(Error::InvalidInput)?
                .unwrap_or_default();

            let depends_on_ids = input
                .depends_on
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.parse::<TaskId>())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| Error::InvalidInput(e.to_string()))?;

            if depends_on_ids.contains(&parent_id) {
                return Err(Error::Conflict(format!(
                    "subtask depends on parent {parent_id}, which would create a cycle"
                )));
            }

            specs.push(SubtaskSpec {
                title: input.title,
                description: input.description,
                acceptance_criteria: input.acceptance_criteria,
                priority,
                assigned_roles: input.assigned_roles.unwrap_or_default(),
                depends_on_ids,
            });
        }

        let mut children = Vec::with_capacity(specs.len());
        for spec in specs {
            let is_blocked = !spec.depends_on_ids.is_empty();
            let mut task = Task::new(
                parent.org_id().clone(),
                parent.project().clone(),
                parent.namespace().clone(),
                spec.title,
                spec.description,
                spec.acceptance_criteria,
                spec.priority,
                spec.assigned_roles,
                created_by.clone(),
                is_blocked,
            )?;
            self.tasks.save(&mut task).await?;

            let mut spawns_edge = Edge::new(
                parent.org_id().clone(),
                ResourceKind::Task,
                parent_id.to_string(),
                ResourceKind::Task,
                task.id().to_string(),
                RelationType::Spawns,
                created_by.clone(),
            )?
            .with_source(ResourceKind::Task, parent_id.to_string());
            self.edges.save(&mut spawns_edge).await?;

            for dep_id in &spec.depends_on_ids {
                let already_exists = self
                    .edges
                    .exists_by_pair(
                        parent.org_id(),
                        &ResourceKind::Task,
                        &task.id().to_string(),
                        &ResourceKind::Task,
                        &dep_id.to_string(),
                        &RelationType::DependsOn,
                    )
                    .await?;
                if !already_exists {
                    let mut dep_edge = Edge::new(
                        parent.org_id().clone(),
                        ResourceKind::Task,
                        task.id().to_string(),
                        ResourceKind::Task,
                        dep_id.to_string(),
                        RelationType::DependsOn,
                        created_by.clone(),
                    )?;
                    self.edges.save(&mut dep_edge).await?;
                }
            }

            children.push(task);
        }

        parent.block()?;
        self.tasks.save(&mut parent).await?;

        Ok((
            TaskDto::from(&parent),
            children.iter().map(TaskDto::from).collect(),
        ))
    }
}
