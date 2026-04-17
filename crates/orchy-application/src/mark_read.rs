use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{MessageId, MessageStore};

pub struct MarkReadCommand {
    pub agent_id: String,
    pub message_ids: Vec<String>,
}

pub struct MarkRead {
    messages: Arc<dyn MessageStore>,
}

impl MarkRead {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, cmd: MarkReadCommand) -> Result<()> {
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;

        let message_ids = cmd
            .message_ids
            .iter()
            .map(|s| s.parse::<MessageId>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        for id in &message_ids {
            if let Some(mut msg) = self.messages.find_by_id(id).await? {
                if msg.is_directed_to(&agent_id) {
                    msg.mark_read()?;
                    self.messages.save(&mut msg).await?;
                    continue;
                }

                if msg.is_broadcast() || msg.is_role_targeted() {
                    self.messages.mark_read_for_agent(id, &agent_id).await?;
                }
            }
        }

        Ok(())
    }
}
