use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::parse_namespace;

use crate::dto::TaskResponse;

pub struct PostTaskCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub parent_id: Option<String>,
    pub created_by: Option<String>,
}

pub struct PostTask {
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
}

impl PostTask {
    pub fn new(tasks: Arc<dyn TaskStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { tasks, edges }
    }

    pub async fn execute(&self, cmd: PostTaskCommand) -> Result<TaskResponse> {
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

        let depends_on = cmd
            .depends_on
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.parse::<TaskId>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let parent_id = cmd
            .parent_id
            .map(|s| s.parse::<TaskId>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let created_by = cmd.created_by.map(|s| AgentId::from_str(&s)).transpose()?;

        let is_blocked = !depends_on.is_empty();

        let mut task = Task::new(
            org_id.clone(),
            project,
            namespace,
            parent_id,
            cmd.title,
            cmd.description,
            cmd.acceptance_criteria,
            priority,
            assigned_roles,
            depends_on.clone(),
            created_by.clone(),
            is_blocked,
        )?;

        self.tasks.save(&mut task).await?;

        for dep_id in &depends_on {
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
            }
        }

        if let Some(pid) = task.parent_id() {
            let already_exists = self
                .edges
                .exists_by_pair(
                    &org_id,
                    &ResourceKind::Task,
                    &pid.to_string(),
                    &ResourceKind::Task,
                    &task.id().to_string(),
                    &RelationType::Spawns,
                )
                .await?;
            if !already_exists {
                let mut edge = Edge::new(
                    org_id,
                    ResourceKind::Task,
                    pid.to_string(),
                    ResourceKind::Task,
                    task.id().to_string(),
                    RelationType::Spawns,
                    created_by,
                )?;
                self.edges.save(&mut edge).await?;
            }
        }

        Ok(TaskResponse::from(&task))
    }
}
