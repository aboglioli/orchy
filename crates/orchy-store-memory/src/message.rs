use async_trait::async_trait;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

use crate::MemoryBackend;

#[async_trait]
impl MessageStore for MemoryBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        {
            let mut messages = self
                .messages
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            messages.insert(message.id(), message.clone());
        }

        let events = message.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(messages.get(id).cloned())
    }

    async fn mark_read_for_agent(&self, message_id: &MessageId, agent: &AgentId) -> Result<()> {
        self.message_receipts
            .write()
            .map_err(|e| Error::Store(e.to_string()))?
            .insert((*message_id, agent.clone()));
        Ok(())
    }

    async fn find_pending(
        &self,
        agent: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let receipts = self
            .message_receipts
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results = Vec::new();

        for msg in messages.values() {
            if msg.status() != MessageStatus::Pending {
                continue;
            }

            let targets_agent = match msg.to() {
                MessageTarget::Agent(id) => id == agent,
                MessageTarget::Broadcast => msg.from() != agent,
                MessageTarget::Role(_) => false,
            };

            if !targets_agent {
                continue;
            }

            if msg.org_id() != org {
                continue;
            }

            if msg.project() != project {
                continue;
            }

            if !msg.namespace().starts_with(namespace) {
                continue;
            }

            if msg.is_broadcast() && receipts.contains(&(msg.id(), agent.clone())) {
                continue;
            }

            results.push(msg.clone());
        }

        Ok(crate::apply_cursor_pagination(results, &page, |m| {
            m.id().to_string()
        }))
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let messages = self
            .messages
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results: Vec<Message> = messages
            .values()
            .filter(|msg| {
                msg.from() == sender
                    && msg.org_id() == org
                    && msg.project() == project
                    && msg.namespace().starts_with(namespace)
            })
            .cloned()
            .collect();

        results.sort_by_key(|m| std::cmp::Reverse(m.created_at()));

        Ok(crate::apply_cursor_pagination(results, &page, |m| {
            m.id().to_string()
        }))
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

        let start = match messages.get(message_id) {
            Some(m) => m.clone(),
            None => return Ok(vec![]),
        };

        let mut root_id = start.id();
        loop {
            let Some(msg) = messages.get(&root_id) else {
                break;
            };
            match msg.reply_to() {
                Some(parent_id) if messages.contains_key(&parent_id) => {
                    root_id = parent_id;
                }
                _ => break,
            }
        }

        let mut thread: Vec<Message> = Vec::new();
        let mut frontier = vec![root_id];
        if let Some(root_msg) = messages.get(&root_id) {
            thread.push(root_msg.clone());
        }

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

        thread.sort_by_key(|a| a.created_at());

        if let Some(n) = limit {
            let len = thread.len();
            if len > n {
                thread = thread.split_off(len - n);
            }
        }

        Ok(thread)
    }
}
