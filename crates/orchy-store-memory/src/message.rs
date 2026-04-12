use chrono::Utc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{
    CreateMessage, Message, MessageId, MessageStatus, MessageStore, MessageTarget,
};
use orchy_core::namespace::Namespace;

use crate::MemoryBackend;

impl MessageStore for MemoryBackend {
    async fn send(&self, cmd: CreateMessage) -> Result<Message> {
        let message = Message {
            id: MessageId::new(),
            namespace: cmd.namespace,
            from: cmd.from,
            to: cmd.to,
            body: cmd.body,
            status: MessageStatus::Pending,
            created_at: Utc::now(),
        };

        let mut messages = self
            .messages
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        messages.insert(message.id, message.clone());
        Ok(message)
    }

    async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        let mut messages = self
            .messages
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results = Vec::new();

        for msg in messages.values_mut() {
            if msg.status != MessageStatus::Pending {
                continue;
            }

            let targets_agent = match &msg.to {
                MessageTarget::Agent(id) => id == agent,
                MessageTarget::Broadcast => true,
                MessageTarget::Role(_) => false,
            };

            if !targets_agent {
                continue;
            }

            if !msg.namespace.starts_with(namespace) {
                continue;
            }

            msg.status = MessageStatus::Delivered;
            results.push(msg.clone());
        }

        Ok(results)
    }

    async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        let mut messages = self
            .messages
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;

        for id in ids {
            if let Some(msg) = messages.get_mut(id) {
                msg.status = MessageStatus::Read;
            }
        }

        Ok(())
    }
}
