use std::sync::Arc;

use orchy_core::{Clock, Id, Namespace, Priority, Role, Tag, TaskStore, Title};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateTaskCommand {
    pub task_id: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub acceptance_criteria: Option<String>,
    pub priority: Option<String>,
    pub roles: Option<Vec<String>>,
    pub namespace: Option<String>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
}

pub struct UpdateTask {
    tasks: Arc<dyn TaskStore>,
    clock: Arc<dyn Clock>,
}

impl UpdateTask {
    pub fn new(tasks: Arc<dyn TaskStore>, clock: Arc<dyn Clock>) -> Self {
        Self { tasks, clock }
    }

    pub async fn execute(&self, cmd: UpdateTaskCommand) -> ApplicationResult<TaskDto> {
        let mut task = self.tasks.require(&Id::new(&cmd.task_id)?).await?;

        if let Some(title) = &cmd.title {
            task.retitle(Title::new(title)?, &*self.clock);
        }
        if let Some(description) = cmd.description {
            task.describe(description, &*self.clock);
        }
        if cmd.acceptance_criteria.is_some() {
            task.set_acceptance_criteria(cmd.acceptance_criteria, &*self.clock);
        }
        if let Some(priority) = &cmd.priority {
            task.set_priority(priority.parse::<Priority>()?, &*self.clock);
        }
        if let Some(roles) = &cmd.roles {
            let roles = roles
                .iter()
                .map(Role::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            task.assign_roles(roles, &*self.clock);
        }
        if let Some(namespace) = &cmd.namespace {
            task.move_to(Namespace::new(namespace)?, &*self.clock);
        }
        if !cmd.add_tags.is_empty() || !cmd.remove_tags.is_empty() {
            let add = cmd
                .add_tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            let remove = cmd
                .remove_tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            task.retag(add, &remove, &*self.clock);
        }

        self.tasks.save(&mut task).await?;
        Ok(TaskDto::from(&task))
    }
}
