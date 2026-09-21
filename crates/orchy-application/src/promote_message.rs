use std::sync::Arc;

use orchy_core::{
    ActorId, Clock, Edge, EdgeStore, EntityRef, Id, IdGenerator, MessageStore, Relation, Role,
    Task, TaskStore, Title,
};
use serde::{Deserialize, Serialize};

use crate::dto::{MessageDto, TaskDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromoteMessageCommand {
    pub message_id: String,
    pub actor: String,
    pub title: Option<String>,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoteMessageResponse {
    pub task: TaskDto,
    pub message: MessageDto,
}

pub struct PromoteMessage {
    messages: Arc<dyn MessageStore>,
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl PromoteMessage {
    pub fn new(
        messages: Arc<dyn MessageStore>,
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            messages,
            tasks,
            edges,
            ids,
            clock,
        }
    }

    pub async fn execute(
        &self,
        cmd: PromoteMessageCommand,
    ) -> ApplicationResult<PromoteMessageResponse> {
        let actor: ActorId = cmd.actor.parse()?;
        let message_id = Id::new(&cmd.message_id)?;
        let message = self.messages.require(&message_id).await?;

        let title = match (&cmd.title, message.subject()) {
            (Some(title), _) => Title::new(title)?,
            (None, Some(subject)) => subject.clone(),
            (None, None) => Title::new(truncate(message.body().as_str()))?,
        };

        let mut task = Task::create(title, message.namespace().clone(), &*self.ids, &*self.clock);
        task.describe(message.body().as_str().to_owned(), &*self.clock);
        if !cmd.roles.is_empty() {
            let roles = cmd
                .roles
                .iter()
                .map(Role::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            task.assign_roles(roles, &*self.clock);
        }
        self.tasks.save(&mut task).await?;

        self.edges
            .add(&Edge::new(
                EntityRef::task(task.id().clone()),
                EntityRef::message(message_id),
                Relation::SpawnedBy,
            )?)
            .await?;

        let mut root = self.messages.require(message.thread()).await?;
        if root.resolve(actor, &*self.clock).is_ok() {
            self.messages.save(&mut root).await?;
        }

        Ok(PromoteMessageResponse {
            task: TaskDto::from(&task),
            message: MessageDto::from(&root),
        })
    }
}

fn truncate(body: &str) -> String {
    let line = body.lines().next().unwrap_or("untitled").trim();
    line.chars().take(120).collect()
}
