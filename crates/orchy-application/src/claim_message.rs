use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Resource};
use orchy_core::message::{MessageId, MessageStore};

pub struct ClaimMessage {
    messages: Arc<dyn MessageStore>,
}

impl ClaimMessage {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, agent_id: AgentId, message_id: MessageId) -> ApplicationResult<()> {
        let mut msg = self
            .messages
            .find_by_id(&message_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Message,
                id: message_id.to_string(),
            })?;
        msg.claim(agent_id)?;
        self.messages.save(&mut msg).await.map_err(Into::into)
    }
}
