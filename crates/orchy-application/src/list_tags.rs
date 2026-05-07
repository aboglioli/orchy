use std::collections::BTreeSet;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::Namespace;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStore};

pub struct ListTagsCommand {
    pub org_id: Option<String>,
    pub project: Option<String>,
    pub namespace: Option<String>,
}

pub struct ListTags {
    tasks: Arc<dyn TaskStore>,
}

impl ListTags {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ListTagsCommand) -> ApplicationResult<Vec<String>> {
        let org_id = cmd.org_id.map(|s| OrganizationId::new(&s)).transpose()?;

        let project = cmd.project.map(ProjectId::try_from).transpose()?;

        let namespace = cmd
            .namespace
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(Namespace::new)
            .transpose()?;

        let filter = TaskFilter {
            org_id,
            project,
            namespace,
            include_archived: None,
            ..Default::default()
        };

        let tasks = self
            .tasks
            .list(filter, PageParams::unbounded())
            .await?
            .items;
        let mut tags = BTreeSet::new();
        for task in &tasks {
            for tag in task.tags() {
                tags.insert(tag.clone());
            }
        }
        Ok(tags.into_iter().collect())
    }
}
