use std::collections::BTreeSet;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::task::{TaskFilter, TaskStore};

use crate::parse_namespace;

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

    pub async fn execute(&self, cmd: ListTagsCommand) -> Result<Vec<String>> {
        let org_id = cmd
            .org_id
            .map(|s| OrganizationId::new(&s))
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let project = cmd
            .project
            .map(ProjectId::try_from)
            .transpose()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let filter = TaskFilter {
            org_id,
            project,
            namespace,
            ..Default::default()
        };

        let tasks = self.tasks.list(filter).await?;
        let mut tags = BTreeSet::new();
        for task in &tasks {
            for tag in task.tags() {
                tags.insert(tag.clone());
            }
        }
        Ok(tags.into_iter().collect())
    }
}
