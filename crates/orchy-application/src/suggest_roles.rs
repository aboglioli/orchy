use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::namespace::Namespace;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;
use orchy_core::task::{TaskFilter, TaskStatus, TaskStore};

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

    pub async fn execute(&self, cmd: SuggestRolesCommand) -> ApplicationResult<Vec<String>> {
        let org_id = cmd.org_id.map(|s| OrganizationId::new(&s)).transpose()?;

        let project = ProjectId::try_from(cmd.project)?;
        let namespace = cmd
            .namespace
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(Namespace::new)
            .transpose()?;

        let mut role_counts: HashMap<String, usize> = HashMap::new();

        for status in &[TaskStatus::Pending, TaskStatus::Blocked] {
            let filter = TaskFilter {
                org_id: org_id.clone(),
                project: Some(project.clone()),
                namespace: namespace.clone(),
                status: Some(*status),
                include_archived: None,
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
        roles.sort_by_key(|a| std::cmp::Reverse(a.1));

        Ok(roles.into_iter().take(3).map(|(r, _)| r).collect())
    }
}
