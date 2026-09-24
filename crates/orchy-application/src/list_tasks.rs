use std::sync::Arc;

use orchy_core::{
    ActorId, Id, Namespace, PageRequest, Role, Tag, TaskQuery, TaskStatus, TaskStore,
};
use serde::{Deserialize, Serialize};

use crate::dto::{PageDto, TaskDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListTasksCommand {
    pub status: Vec<String>,
    pub namespace: Option<String>,
    pub claimed_by: Option<String>,
    pub role: Option<String>,
    pub parent: Option<String>,
    pub tags: Vec<String>,
    pub text: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub struct ListTasks {
    tasks: Arc<dyn TaskStore>,
}

impl ListTasks {
    pub fn new(tasks: Arc<dyn TaskStore>) -> Self {
        Self { tasks }
    }

    pub async fn execute(&self, cmd: ListTasksCommand) -> ApplicationResult<PageDto<TaskDto>> {
        let query = TaskQuery {
            status: if cmd.status.is_empty() {
                None
            } else {
                Some(
                    cmd.status
                        .iter()
                        .map(|s| s.parse::<TaskStatus>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
            claimed_by: cmd
                .claimed_by
                .as_deref()
                .map(str::parse::<ActorId>)
                .transpose()?,
            role: cmd.role.as_deref().map(Role::new).transpose()?,
            parent: cmd.parent.as_deref().map(Id::new).transpose()?,
            tags: cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?,
            text: cmd.text,
        };
        let page = PageRequest::new(
            cmd.offset.unwrap_or(0),
            cmd.limit.unwrap_or(PageRequest::default().limit()),
        );
        let found = self.tasks.find(&query, page).await?;
        Ok(PageDto::new(
            found.items.iter().map(TaskDto::from).collect(),
            found.total,
            found.offset,
            found.limit,
        ))
    }
}
