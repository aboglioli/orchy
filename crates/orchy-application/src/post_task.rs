use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskId, TaskStatus, TaskStore};

use crate::dto::TaskDto;
use crate::parse_namespace;

pub struct PostTaskCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub created_by: Option<String>,
    pub parent_id: Option<String>,
    pub depends_on: Option<Vec<String>>,
}

pub struct PostTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl PostTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: PostTaskCommand) -> Result<TaskDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let priority = cmd
            .priority
            .map(|p| p.parse::<Priority>())
            .transpose()
            .map_err(Error::InvalidInput)?
            .unwrap_or_default();

        let assigned_roles = cmd.assigned_roles.unwrap_or_default();

        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

        let mut task = Task::new(
            org_id.clone(),
            project,
            namespace,
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
            assigned_roles,
            created_by.clone(),
            false,
        )?;

        self.tasks.save(&mut task).await?;

        if let Some(parent_id_str) = cmd.parent_id {
            let parent_id = parent_id_str
                .parse::<TaskId>()
                .map_err(|e| Error::InvalidInput(e.to_string()))?;
            let parent = self
                .tasks
                .find_by_id(&parent_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("parent task {parent_id}")))?;
            if parent.org_id() != &org_id {
                return Err(Error::NotFound(format!("parent task {parent_id}")));
            }

            let mut edge = Edge::new(
                org_id.clone(),
                ResourceKind::Task,
                parent_id.to_string(),
                ResourceKind::Task,
                task.id().to_string(),
                RelationType::Spawns,
                created_by.clone(),
            )?;
            self.edges.save(&mut edge).await?;
        }

        let mut has_incomplete_dep = false;
        if let Some(dep_ids) = cmd.depends_on {
            for dep_id_str in dep_ids {
                let dep_id = dep_id_str
                    .parse::<TaskId>()
                    .map_err(|e| Error::InvalidInput(e.to_string()))?;
                let dep = self
                    .tasks
                    .find_by_id(&dep_id)
                    .await?
                    .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
                if dep.org_id() != &org_id {
                    return Err(Error::NotFound(format!("dependency task {dep_id}")));
                }

                let mut edge = Edge::new(
                    org_id.clone(),
                    ResourceKind::Task,
                    task.id().to_string(),
                    ResourceKind::Task,
                    dep_id.to_string(),
                    RelationType::DependsOn,
                    created_by.clone(),
                )?;
                self.edges.save(&mut edge).await?;

                if dep.status() != TaskStatus::Completed {
                    has_incomplete_dep = true;
                }
            }
        }

        if has_incomplete_dep {
            task.block()?;
            self.tasks.save(&mut task).await?;
        }

        Ok(TaskDto::from(&task))
    }
}
