use std::sync::Arc;

use orchy_core::{ActorId, Id, MessageStore, ReadWatermarks};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadMessageCommand {
    pub message_id: String,
    pub actor: String,
}

pub struct ReadMessage {
    messages: Arc<dyn MessageStore>,
    watermarks: Arc<dyn ReadWatermarks>,
}

impl ReadMessage {
    pub fn new(messages: Arc<dyn MessageStore>, watermarks: Arc<dyn ReadWatermarks>) -> Self {
        Self {
            messages,
            watermarks,
        }
    }

    pub async fn execute(&self, cmd: ReadMessageCommand) -> ApplicationResult<MessageDto> {
        let id = Id::new(&cmd.message_id)?;
        let actor: ActorId = cmd.actor.parse()?;
        let message = self.messages.require(&id).await?;

        let current = self.watermarks.watermark(&actor)?;
        if current.as_ref().is_none_or(|mark| &id > mark) {
            self.watermarks.advance(&actor, &id)?;
        }

        Ok(MessageDto::from(&message))
    }
}
