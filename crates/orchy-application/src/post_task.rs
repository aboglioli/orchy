use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{Priority, Task, TaskId, TaskStore};

use crate::parse_namespace;

use crate::dto::TaskResponse;

pub struct PostTaskCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub title: String,
    pub description: String,
    pub priority: Option<String>,
    pub assigned_roles: Option<Vec<String>>,
    pub depends_on: Option<Vec<String>>,
    pub parent_id: Option<String>,
    pub created_by: Option<String>,
}

pub struct PostTask {
    tasks: Arc<dyn TaskStore>,
}

impl PostTask {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
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

        let created_by = cmd
            .created_by
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let is_blocked = !depends_on.is_empty();

        let mut task = Task::new(
            org_id,
            project,
            namespace,
            parent_id,
            cmd.title,
            cmd.description,
            priority,
            assigned_roles,
            depends_on,
            created_by,
            is_blocked,
        )?;

        self.tasks.save(&mut task).await?;
        Ok(TaskResponse::from(&task))
    }
}
