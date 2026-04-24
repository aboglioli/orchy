use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::message::{MessageId, MessageStore};

pub struct MarkReadCommand {
    pub agent_id: String,
    pub message_ids: Vec<String>,
}

pub struct MarkRead {
    messages: Arc<dyn MessageStore>,
    agents: Arc<dyn AgentStore>,
}

impl MarkRead {
    pub fn new(messages: Arc<dyn MessageStore>, agents: Arc<dyn AgentStore>) -> Self {
        Self { messages, agents }
    }

    pub async fn execute(&self, cmd: MarkReadCommand) -> Result<()> {
        let agent_id = AgentId::from_str(&cmd.agent_id)?;
        self.agents
            .find_by_id(&agent_id)
            .await?
            .ok_or(Error::NotFound("agent not found".to_string()))?;

        let message_ids = cmd
            .message_ids
            .iter()
            .map(|s| s.parse::<MessageId>())
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut receipt_ids = Vec::new();

        for id in &message_ids {
            let Some(mut msg) = self.messages.find_by_id(id).await? else {
                return Err(Error::NotFound(format!("message {id}")));
            };

            if msg.is_directed_to(&agent_id) {
                msg.mark_read()?;
                self.messages.save(&mut msg).await?;
                continue;
            }

            if msg.is_broadcast() || msg.is_role_targeted() || msg.is_namespace_targeted() {
                receipt_ids.push(*id);
            }
        }

        if !receipt_ids.is_empty() {
            self.messages.mark_read(&agent_id, &receipt_ids).await?;
        }

        Ok(())
    }
}
