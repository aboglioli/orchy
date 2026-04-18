use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::message::{MessageId, MessageStore};

use crate::dto::MessageResponse;

pub struct ListConversationCommand {
    pub message_id: String,
    pub limit: Option<u32>,
}

pub struct ListConversation {
    messages: Arc<dyn MessageStore>,
}

impl ListConversation {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, cmd: ListConversationCommand) -> Result<Vec<MessageResponse>> {
        let message_id = cmd
            .message_id
            .parse::<MessageId>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let limit = cmd.limit.map(|l| l as usize);
        let messages = self.messages.find_thread(&message_id, limit).await?;
        Ok(messages.iter().map(MessageResponse::from).collect())
    }
}
