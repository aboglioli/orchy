use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::agent::{Agent, AgentId, AgentStore};
use crate::error::Result;
use crate::knowledge::{Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeStore};
use crate::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use crate::namespace::{Namespace, NamespaceStore, ProjectId};
use crate::organization::{Organization, OrganizationId, OrganizationStore};
use crate::pagination::{Page, PageParams};
use crate::project::{Project, ProjectStore};
use crate::resource_lock::{LockStore, ResourceLock};
use crate::task::{
    ReviewId, ReviewRequest, ReviewStore, Task, TaskFilter, TaskId, TaskStore, TaskWatcher,
    WatcherStore,
};

#[derive(Debug, Default)]
pub struct MockStore {
    agents: RwLock<HashMap<AgentId, Agent>>,
    messages: RwLock<HashMap<MessageId, Message>>,
    message_receipts: RwLock<HashSet<(MessageId, AgentId)>>,
}

#[async_trait::async_trait]
impl OrganizationStore for MockStore {
    async fn save(&self, _: &mut Organization) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &OrganizationId) -> Result<Option<Organization>> {
        Ok(None)
    }
    async fn find_by_api_key(&self, _: &str) -> Result<Option<Organization>> {
        Ok(None)
    }
    async fn list(&self) -> Result<Vec<Organization>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl TaskStore for MockStore {
    async fn save(&self, _: &mut Task) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &TaskId) -> Result<Option<Task>> {
        unimplemented!()
    }
    async fn list(&self, _: TaskFilter, _: PageParams) -> Result<Page<Task>> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl AgentStore for MockStore {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        self.agents
            .write()
            .unwrap()
            .insert(agent.id().clone(), agent.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        Ok(self.agents.read().unwrap().get(id).cloned())
    }
    async fn list(&self, _org: &OrganizationId, _page: PageParams) -> Result<Page<Agent>> {
        let items: Vec<Agent> = self.agents.read().unwrap().values().cloned().collect();
        Ok(Page::new(items, None))
    }
    async fn find_timed_out(&self, _: u64) -> Result<Vec<Agent>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl MessageStore for MockStore {
    async fn save(&self, message: &mut Message) -> Result<()> {
        self.messages
            .write()
            .unwrap()
            .insert(message.id(), message.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        Ok(self.messages.read().unwrap().get(id).cloned())
    }
    async fn mark_read_for_agent(&self, message_id: &MessageId, agent: &AgentId) -> Result<()> {
        self.message_receipts
            .write()
            .unwrap()
            .insert((*message_id, agent.clone()));
        Ok(())
    }
    async fn find_pending(
        &self,
        agent: &AgentId,
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        _page: PageParams,
    ) -> Result<Page<Message>> {
        let receipts = self.message_receipts.read().unwrap();
        let items: Vec<Message> = self
            .messages
            .read()
            .unwrap()
            .values()
            .filter(|m| {
                m.status() == MessageStatus::Pending
                    && match m.to() {
                        MessageTarget::Agent(id) => id == agent,
                        MessageTarget::Broadcast => {
                            m.from() != agent && !receipts.contains(&(m.id(), agent.clone()))
                        }
                        _ => false,
                    }
                    && m.project() == project
                    && m.namespace().starts_with(namespace)
            })
            .cloned()
            .collect();
        Ok(Page::new(items, None))
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        _org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        _page: PageParams,
    ) -> Result<Page<Message>> {
        let items: Vec<Message> = self
            .messages
            .read()
            .unwrap()
            .values()
            .filter(|m| {
                m.from() == sender && m.project() == project && m.namespace().starts_with(namespace)
            })
            .cloned()
            .collect();
        Ok(Page::new(items, None))
    }

    async fn find_thread(
        &self,
        _message_id: &MessageId,
        _limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl ProjectStore for MockStore {
    async fn save(&self, _: &mut Project) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &OrganizationId, _: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

#[async_trait::async_trait]
impl NamespaceStore for MockStore {
    async fn register(&self, _: &OrganizationId, _: &ProjectId, _: &Namespace) -> Result<()> {
        Ok(())
    }
    async fn list(&self, _: &OrganizationId, _: &ProjectId) -> Result<Vec<Namespace>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl LockStore for MockStore {
    async fn save(&self, _: &mut ResourceLock) -> Result<()> {
        unimplemented!()
    }
    async fn find(
        &self,
        _: &OrganizationId,
        _: &ProjectId,
        _: &Namespace,
        _: &str,
    ) -> Result<Option<ResourceLock>> {
        unimplemented!()
    }
    async fn delete(
        &self,
        _: &OrganizationId,
        _: &ProjectId,
        _: &Namespace,
        _: &str,
    ) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_holder(&self, _: &AgentId) -> Result<Vec<ResourceLock>> {
        Ok(vec![])
    }
    async fn delete_expired(&self) -> Result<u64> {
        unimplemented!()
    }
}

#[async_trait::async_trait]
impl WatcherStore for MockStore {
    async fn save(&self, _: &mut TaskWatcher) -> Result<()> {
        Ok(())
    }
    async fn delete(&self, _: &TaskId, _: &AgentId) -> Result<()> {
        Ok(())
    }
    async fn find_watchers(&self, _: &TaskId) -> Result<Vec<TaskWatcher>> {
        Ok(vec![])
    }
    async fn find_by_agent(&self, _: &AgentId) -> Result<Vec<TaskWatcher>> {
        Ok(vec![])
    }
}

#[async_trait::async_trait]
impl ReviewStore for MockStore {
    async fn save(&self, _: &mut ReviewRequest) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &ReviewId) -> Result<Option<ReviewRequest>> {
        unimplemented!()
    }
    async fn find_pending_for_agent(&self, _: &AgentId) -> Result<Vec<ReviewRequest>> {
        Ok(vec![])
    }
    async fn find_by_task(&self, _: &TaskId, _: PageParams) -> Result<Page<ReviewRequest>> {
        Ok(Page::empty())
    }
}

#[async_trait::async_trait]
impl KnowledgeStore for MockStore {
    async fn save(&self, _: &mut Knowledge) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &KnowledgeId) -> Result<Option<Knowledge>> {
        unimplemented!()
    }
    async fn find_by_path(
        &self,
        _: &OrganizationId,
        _: Option<&ProjectId>,
        _: &Namespace,
        _: &str,
    ) -> Result<Option<Knowledge>> {
        unimplemented!()
    }
    async fn list(&self, _: KnowledgeFilter, _: PageParams) -> Result<Page<Knowledge>> {
        unimplemented!()
    }
    async fn search(
        &self,
        _: &OrganizationId,
        _: &str,
        _: Option<&[f32]>,
        _: Option<&Namespace>,
        _: usize,
    ) -> Result<Vec<Knowledge>> {
        unimplemented!()
    }
    async fn delete(&self, _: &KnowledgeId) -> Result<()> {
        unimplemented!()
    }
}
