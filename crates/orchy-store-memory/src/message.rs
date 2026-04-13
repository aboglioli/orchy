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

    async fn find_sent(
        &self,
        sender: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results: Vec<Message> = messages
            .values()
            .filter(|msg| {
                msg.from() == *sender
                    && msg.project() == project
                    && msg.namespace().starts_with(namespace)
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.created_at().cmp(&a.created_at()));
        Ok(results)
    }

    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        // Find the starting message
        let start = match messages.get(message_id) {
            Some(m) => m.clone(),
            None => return Ok(vec![]),
        };

        // Walk backwards to find the root
        let mut root_id = start.id();
        loop {
            let msg = messages.get(&root_id).unwrap();
            match msg.reply_to() {
                Some(parent_id) if messages.contains_key(&parent_id) => {
                    root_id = parent_id;
                }
                _ => break,
            }
        }

        // Collect all messages in the thread (connected via reply_to to root)
        let mut thread: Vec<Message> = Vec::new();
        let mut frontier = vec![root_id];
        thread.push(messages.get(&root_id).unwrap().clone());

        while !frontier.is_empty() {
            let mut next_frontier = Vec::new();
            for parent_id in &frontier {
                for msg in messages.values() {
                    if msg.reply_to() == Some(*parent_id) {
                        thread.push(msg.clone());
                        next_frontier.push(msg.id());
                    }
                }
            }
            frontier = next_frontier;
        }

        thread.sort_by(|a, b| a.created_at().cmp(&b.created_at()));

        if let Some(n) = limit {
            let len = thread.len();
            if len > n {
                thread = thread.split_off(len - n);
            }
        }

        Ok(thread)
    }
}
