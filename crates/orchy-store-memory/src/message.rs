use async_trait::async_trait;

use orchy_core::agent::AgentId;
use orchy_core::error::Result;
use orchy_core::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

use crate::MemoryBackend;

#[async_trait]
impl MessageStore for MemoryBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        let is_new = {
            let mut messages = self.messages.write().await;
            let is_new = !messages.contains_key(&message.id());
            messages.insert(message.id(), message.clone());
            is_new
        };

        if is_new {
            if let Some(parent_id) = message.reply_to() {
                self.message_replies
                    .write()
                    .await
                    .entry(parent_id)
                    .or_default()
                    .push(message.id());
            }
        }

        let events = message.drain_events();
        if !events.is_empty() {
            if let Err(e) = orchy_events::io::Writer::write_all(self, &events).await {
                tracing::error!("failed to persist events: {e}");
            }
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        let messages = self.messages.read().await;
        Ok(messages.get(id).cloned())
    }

    async fn mark_read_for_agent(&self, message_id: &MessageId, agent: &AgentId) -> Result<()> {
        self.message_receipts
            .write()
            .await
            .insert((*message_id, agent.clone()));
        Ok(())
    }

    async fn find_pending(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        let messages = self.messages.read().await;
        let receipts = self.message_receipts.read().await;

        let mut results = Vec::new();

        for msg in messages.values() {
            if msg.status() != MessageStatus::Pending {
                continue;
            }

            let targets_agent = match msg.to() {
                MessageTarget::Agent(id) => id == agent,
                MessageTarget::Broadcast => msg.from() != agent,
                MessageTarget::Role(role) => msg.from() != agent && agent_roles.contains(role),
                MessageTarget::Namespace(_) => false,
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

            if (msg.is_broadcast() || msg.is_role_targeted())
                && receipts.contains(&(msg.id(), agent.clone()))
            {
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
        let messages = self.messages.read().await;

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
        let messages = self.messages.read().await;
        let replies = self.message_replies.read().await;

        let start = match messages.get(message_id) {
            Some(m) => m.clone(),
            None => return Ok(vec![]),
        };

        let mut root_id = start.id();
        while let Some(msg) = messages.get(&root_id) {
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
                if let Some(children) = replies.get(parent_id) {
                    for child_id in children {
                        if let Some(msg) = messages.get(child_id) {
                            thread.push(msg.clone());
                            next_frontier.push(*child_id);
                        }
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

    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>> {
        let messages = self.messages.read().await;
        Ok(ids
            .iter()
            .filter_map(|id| messages.get(id))
            .cloned()
            .collect())
    }
}
