use std::sync::Arc;

use orchy_core::{Id, MessageStore};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadThreadCommand {
    pub message_id: String,
}

pub struct ReadThread {
    messages: Arc<dyn MessageStore>,
}

impl ReadThread {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, cmd: ReadThreadCommand) -> ApplicationResult<Vec<MessageDto>> {
        let id = Id::new(&cmd.message_id)?;
        let anchor = self.messages.require(&id).await?;
        let thread = self.messages.thread(anchor.thread()).await?;
        Ok(thread.iter().map(MessageDto::from).collect())
    }
}
