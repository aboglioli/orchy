use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::dto::{PageResponse, TaskResponse};
use crate::parse_namespace;

pub struct ListTasksCommand {
    pub org_id: String,
    pub project: Option<String>,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub parent_id: Option<String>,
    pub assigned_to: Option<String>,
    pub tag: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct ListTasks {
    tasks: Arc<dyn TaskStore>,
}

impl ListTasks {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ListTasksCommand) -> Result<PageResponse<TaskResponse>> {
        let org_id =
            Some(OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?);

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let namespace = cmd
            .namespace
            .map(|s| parse_namespace(Some(&s)))
            .transpose()?;

        let status = cmd
            .status
            .map(|s| s.parse::<TaskStatus>())
            .transpose()
            .map_err(Error::InvalidInput)?;

        let parent_id = cmd
            .parent_id
            .map(|s| s.parse::<TaskId>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let assigned_to = cmd
            .assigned_to
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let filter = TaskFilter {
            org_id,
            project,
            namespace,
            status,
            parent_id,
            assigned_to,
            tag: cmd.tag,
            ..Default::default()
        };

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self.tasks.list(filter, page).await?;
        Ok(PageResponse::from(result))
    }
}
