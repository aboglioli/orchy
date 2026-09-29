use std::sync::Arc;

use orchy_core::{ActorId, Id, MessageStore, ReadWatermarks};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadInboxCommand {
    pub actor: String,
    pub all: bool,
    /// Any message of the conversation to keep to.
    pub thread: Option<String>,
}

pub struct ReadInbox {
    messages: Arc<dyn MessageStore>,
    watermarks: Arc<dyn ReadWatermarks>,
}

impl ReadInbox {
    pub fn new(messages: Arc<dyn MessageStore>, watermarks: Arc<dyn ReadWatermarks>) -> Self {
        Self {
            messages,
            watermarks,
        }
    }

    pub async fn execute(&self, cmd: ReadInboxCommand) -> ApplicationResult<Vec<MessageDto>> {
        let actor: ActorId = cmd.actor.parse()?;
        let after = if cmd.all {
            None
        } else {
            self.watermarks.watermark(&actor)?
        };
        let thread = match &cmd.thread {
            Some(message) => Some(
                self.messages
                    .require(&Id::new(message)?)
                    .await?
                    .thread()
                    .clone(),
            ),
            None => None,
        };
        let messages = self.messages.inbox(&actor, after.as_ref()).await?;
        Ok(messages
            .iter()
            .filter(|m| thread.as_ref().is_none_or(|t| m.thread() == t))
            .map(MessageDto::from)
            .collect())
    }
}
