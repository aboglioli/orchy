use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, Clock, Id, IdGenerator, Namespace, Priority, Role, Tag, Task, TaskStore,
    Title, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::TaskDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateTaskCommand {
    pub actor: Option<String>,
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
    actors: Arc<dyn ActorStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl CreateTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        actors: Arc<dyn ActorStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            tasks,
            actors,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: CreateTaskCommand) -> ApplicationResult<TaskDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: CreateTaskCommand) -> ApplicationResult<TaskDto> {
        let title = Title::new(&cmd.title)?;
        let namespace = match (&cmd.namespace, &cmd.actor) {
            (Some(ns), _) => Namespace::new(ns)?,
            (None, Some(actor)) => self.actors.home_of(&actor.parse::<ActorId>()?).await?,
            (None, None) => Namespace::root(),
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
