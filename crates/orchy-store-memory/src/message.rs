use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};

use crate::MemoryBackend;

impl MessageStore for MemoryBackend {
    async fn save(&self, message: &Message) -> Result<()> {
        let mut messages = self
            .messages
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        messages.insert(message.id(), message.clone());
        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(messages.get(id).cloned())
    }

    async fn find_pending(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results = Vec::new();

        for msg in messages.values() {
            if msg.status() != MessageStatus::Pending {
                continue;
            }

            let targets_agent = match msg.to() {
                MessageTarget::Agent(id) => id == agent,
                MessageTarget::Broadcast => true,
                MessageTarget::Role(_) => false,
            };

            if !targets_agent {
                continue;
            }

            if msg.project() != project {
                continue;
            }

            if !msg.namespace().starts_with(namespace) {
                continue;
            }

            results.push(msg.clone());
        }

        Ok(results)
    }
}
