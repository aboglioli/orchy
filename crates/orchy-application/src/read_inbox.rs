use std::sync::Arc;

use orchy_core::{ActorId, MessageStore, ReadWatermarks};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadInboxCommand {
    pub actor: String,
    pub all: bool,
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
        let messages = self.messages.inbox(&actor, after.as_ref()).await?;
        Ok(messages.iter().map(MessageDto::from).collect())
    }
}
