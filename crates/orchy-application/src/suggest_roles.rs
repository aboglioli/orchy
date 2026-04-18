use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

use crate::parse_namespace;

pub struct SuggestRolesCommand {
    pub org_id: Option<String>,
    pub project: String,
    pub namespace: Option<String>,
}

pub struct SuggestRoles {
    tasks: Arc<dyn TaskStore>,
}

impl SuggestRoles {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: SuggestRolesCommand) -> Result<Vec<String>> {
        let org_id = cmd
            .org_id
            .map(|s| OrganizationId::new(&s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let mut role_counts: HashMap<String, usize> = HashMap::new();

        for status in &[TaskStatus::Pending, TaskStatus::Blocked] {
            let filter = TaskFilter {
                org_id: org_id.clone(),
                project: Some(project.clone()),
                namespace: namespace.clone(),
                status: Some(*status),
                ..Default::default()
            };
            let tasks = self
                .tasks
                .list(filter, PageParams::unbounded())
                .await?
                .items;
            for task in &tasks {
                for role in task.assigned_roles() {
                    *role_counts.entry(role.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut roles: Vec<(String, usize)> = role_counts.into_iter().collect();
        roles.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(roles.into_iter().take(3).map(|(r, _)| r).collect())
    }
}
