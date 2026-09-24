use std::sync::Arc;

use orchy_core::{ActorId, MessageStore};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListSentCommand {
    pub actor: String,
}

pub struct ListSent {
    messages: Arc<dyn MessageStore>,
}

impl ListSent {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, cmd: ListSentCommand) -> ApplicationResult<Vec<MessageDto>> {
        let actor: ActorId = cmd.actor.parse()?;
        let messages = self.messages.sent_by(&actor).await?;
        Ok(messages.iter().map(MessageDto::from).collect())
    }
}
