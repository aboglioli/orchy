use std::sync::Arc;

use orchy_core::{Clock, Id, IdGenerator, Namespace, Priority, Role, Tag, Task, TaskStore, Title};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateTaskCommand {
    pub title: String,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub namespace: Option<String>,
    pub roles: Vec<String>,
    pub tags: Vec<String>,
    pub parent: Option<String>,
    pub depends_on: Vec<String>,
}

pub struct CreateTask {
    tasks: Arc<dyn TaskStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl CreateTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self { tasks, ids, clock }
    }

    pub async fn execute(&self, cmd: CreateTaskCommand) -> ApplicationResult<TaskDto> {
        let title = Title::new(&cmd.title)?;
        let namespace = match &cmd.namespace {
            Some(ns) => Namespace::new(ns)?,
            None => Namespace::root(),
        };

        let mut task = Task::create(title, namespace, &*self.ids, &*self.clock);

        if let Some(description) = cmd.description {
            task.describe(description, &*self.clock);
        }
        if cmd.acceptance_criteria.is_some() {
            task.set_acceptance_criteria(cmd.acceptance_criteria, &*self.clock);
        }
        if let Some(priority) = &cmd.priority {
            task.set_priority(priority.parse::<Priority>()?, &*self.clock);
        }
        if !cmd.roles.is_empty() {
            let roles = cmd
                .roles
                .iter()
                .map(Role::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            task.assign_roles(roles, &*self.clock);
        }
        if !cmd.tags.is_empty() {
            let tags = cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            task.retag(tags, &[], &*self.clock);
        }
        if let Some(parent) = &cmd.parent {
            task.attach_to(Id::new(parent)?, &*self.clock)?;
        }
        for dependency in &cmd.depends_on {
            task.add_dependency(Id::new(dependency)?, &*self.clock)?;
        }

        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
