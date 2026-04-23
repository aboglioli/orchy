use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{MessageId, MessageStore};

pub struct ClaimMessage {
    messages: Arc<dyn MessageStore>,
}

impl ClaimMessage {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, agent_id: AgentId, message_id: MessageId) -> Result<()> {
        let mut msg = self
            .messages
            .find_by_id(&message_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("message {message_id}")))?;
        msg.claim(agent_id)?;
        self.messages.save(&mut msg).await
    }
}
