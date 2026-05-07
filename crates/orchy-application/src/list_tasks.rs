use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::AgentId;
use orchy_core::namespace::Namespace;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

use crate::dto::{PageResponse, TaskDto};

pub struct ListTasksCommand {
    pub org_id: String,
    pub project: Option<String>,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub assigned_to: Option<String>,
    pub tag: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub archived: Option<bool>,
}

pub struct ListTasks {
    tasks: Arc<dyn TaskStore>,
}

impl ListTasks {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ListTasksCommand) -> ApplicationResult<PageResponse<TaskDto>> {
        let org_id = Some(OrganizationId::new(&cmd.org_id)?);

        let project = cmd.project.map(ProjectId::try_from).transpose()?;

        let namespace = cmd.namespace.map(Namespace::new).transpose()?;

        let status = cmd.status.map(|s| s.parse::<TaskStatus>()).transpose()?;

        let assigned_to = cmd.assigned_to.map(|s| AgentId::from_str(&s)).transpose()?;

        let filter = TaskFilter {
            org_id,
            project,
            namespace,
            status,
            assigned_to,
            tag: cmd.tag,
            include_archived: cmd.archived,
            ..Default::default()
        };

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.tasks.list(filter, page).await?;
        Ok(PageResponse::from(result))
    }
}
