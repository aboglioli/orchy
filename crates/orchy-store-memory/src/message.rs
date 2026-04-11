use chrono::Utc;

use orchy_core::entities::{CreateMessage, Message, MessageStatus};
use orchy_core::error::{Error, Result};
use orchy_core::store::MessageStore;
use orchy_core::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

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

        let mut messages = self.messages.write().map_err(|e| Error::Store(e.to_string()))?;
        messages.insert(message.id, message.clone());
        Ok(message)
    }

    async fn check(&self, agent: &AgentId, namespace: Option<&Namespace>) -> Result<Vec<Message>> {
        let mut messages = self.messages.write().map_err(|e| Error::Store(e.to_string()))?;

        let mut results = Vec::new();

        for msg in messages.values_mut() {
            if msg.status != MessageStatus::Pending {
                continue;
            }

            // Check if message targets this agent
            let targets_agent = match &msg.to {
                MessageTarget::Agent(id) => id == agent,
                MessageTarget::Broadcast => true,
                MessageTarget::Role(_) => false, // fan-out handled at service level
            };

            if !targets_agent {
                continue;
            }

            // Filter by namespace if provided
            if let Some(ns) = namespace {
                if let Some(ref msg_ns) = msg.namespace {
                    if !msg_ns.starts_with(ns) {
                        continue;
                    }
                } else {
                    continue;
                }
            }

            msg.status = MessageStatus::Delivered;
            results.push(msg.clone());
        }

        Ok(results)
    }

    async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        let mut messages = self.messages.write().map_err(|e| Error::Store(e.to_string()))?;

        for id in ids {
            if let Some(msg) = messages.get_mut(id) {
                msg.status = MessageStatus::Read;
            }
        }

        Ok(())
    }
}
