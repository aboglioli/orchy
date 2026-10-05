use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, Body, Clock, Id, IdGenerator, Message, MessageStore, Namespace, Priority,
    Recipient, Title, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SendMessageCommand {
    pub from: String,
    pub to: Vec<String>,
    pub subject: Option<String>,
    pub body: String,
    pub namespace: Option<String>,
    pub priority: Option<String>,
    pub reply_to: Option<String>,
}

pub struct SendMessage {
    messages: Arc<dyn MessageStore>,
    actors: Arc<dyn ActorStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl SendMessage {
    pub fn new(
        messages: Arc<dyn MessageStore>,
        actors: Arc<dyn ActorStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            messages,
            actors,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: SendMessageCommand) -> ApplicationResult<MessageDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: SendMessageCommand) -> ApplicationResult<MessageDto> {
        let from: ActorId = cmd.from.parse()?;
        let body = Body::new(cmd.body);

        let mut message = match &cmd.reply_to {
            Some(parent) => {
                let parent = self.messages.require(&Id::new(parent)?).await?;
                parent.reply(from, body, &*self.ids, &*self.clock)?
            }
            None => {
                let to = cmd
                    .to
                    .iter()
                    .map(|r| r.parse::<Recipient>())
                    .collect::<orchy_core::Result<Vec<_>>>()?;
                let subject = cmd.subject.as_deref().map(Title::new).transpose()?;
                let namespace = match &cmd.namespace {
                    Some(ns) => Namespace::new(ns)?,
                    None => self.actors.home_of(&from).await?,
                };
                Message::send(from, to, subject, body, namespace, &*self.ids, &*self.clock)?
            }
        };

        if let Some(priority) = &cmd.priority {
            message = message.with_priority(priority.parse::<Priority>()?);
        }

        self.messages.save(&mut message).await?;
        Ok(MessageDto::from(&message))
    }
}
