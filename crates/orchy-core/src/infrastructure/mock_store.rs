use std::collections::HashMap;
use std::sync::RwLock;

use crate::agent::{Agent, AgentId, AgentStore};
use crate::document::{Document, DocumentFilter, DocumentId, DocumentStore};
use crate::error::Result;
use crate::knowledge::{Entry, EntryFilter, EntryId, EntryStore};
use crate::memory::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore};
use crate::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use crate::namespace::{Namespace, NamespaceStore, ProjectId};
use crate::project::{Project, ProjectStore};
use crate::project_link::{ProjectLink, ProjectLinkId, ProjectLinkStore};
use crate::resource_lock::{LockStore, ResourceLock};
use crate::skill::{Skill, SkillFilter, SkillStore};
use crate::task::{
    ReviewId, ReviewRequest, ReviewStore, Task, TaskFilter, TaskId, TaskStore, TaskWatcher,
    WatcherStore,
};

#[derive(Debug, Default)]
pub struct MockStore {
    agents: RwLock<HashMap<AgentId, Agent>>,
    messages: RwLock<HashMap<MessageId, Message>>,
}

impl TaskStore for MockStore {
    async fn save(&self, _: &mut Task) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &TaskId) -> Result<Option<Task>> {
        unimplemented!()
    }
    async fn list(&self, _: TaskFilter) -> Result<Vec<Task>> {
        unimplemented!()
    }
}

impl AgentStore for MockStore {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        self.agents
            .write()
            .unwrap()
            .insert(agent.id(), agent.clone());
        Ok(())
    }
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        Ok(self.agents.read().unwrap().get(id).cloned())
    }
    async fn list(&self) -> Result<Vec<Agent>> {
        Ok(self.agents.read().unwrap().values().cloned().collect())
    }
    async fn find_timed_out(&self, _: u64) -> Result<Vec<Agent>> {
        Ok(vec![])
    }
}

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
    async fn find_pending(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        Ok(self
            .messages
            .read()
            .unwrap()
            .values()
            .filter(|m| {
                m.status() == MessageStatus::Pending
                    && match m.to() {
                        MessageTarget::Agent(id) => id == agent,
                        MessageTarget::Broadcast => true,
                        _ => false,
                    }
                    && m.project() == project
                    && m.namespace().starts_with(namespace)
            })
            .cloned()
            .collect())
    }

    async fn find_sent(
        &self,
        sender: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        Ok(self
            .messages
            .read()
            .unwrap()
            .values()
            .filter(|m| {
                m.from() == *sender
                    && m.project() == project
                    && m.namespace().starts_with(namespace)
            })
            .cloned()
            .collect())
    }

    async fn find_thread(
        &self,
        _message_id: &MessageId,
        _limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        unimplemented!()
    }
}

impl ProjectStore for MockStore {
    async fn save(&self, _: &mut Project) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

impl MemoryStore for MockStore {
    async fn save(&self, _: &mut MemoryEntry) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_key(
        &self,
        _: &ProjectId,
        _: &Namespace,
        _: &str,
    ) -> Result<Option<MemoryEntry>> {
        unimplemented!()
    }
    async fn list(&self, _: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        unimplemented!()
    }
    async fn search(
        &self,
        _: &str,
        _: Option<&[f32]>,
        _: Option<&Namespace>,
        _: usize,
    ) -> Result<Vec<MemoryEntry>> {
        unimplemented!()
    }
    async fn delete(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<()> {
        unimplemented!()
    }
}

impl ContextStore for MockStore {
    async fn save(&self, _: &mut ContextSnapshot) -> Result<()> {
        unimplemented!()
    }
    async fn find_latest(&self, _: &AgentId) -> Result<Option<ContextSnapshot>> {
        unimplemented!()
    }
    async fn list(&self, _: Option<&AgentId>, _: &Namespace) -> Result<Vec<ContextSnapshot>> {
        unimplemented!()
    }
    async fn search(
        &self,
        _: &str,
        _: Option<&[f32]>,
        _: &Namespace,
        _: Option<&AgentId>,
        _: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        unimplemented!()
    }
}

impl NamespaceStore for MockStore {
    async fn register(&self, _: &ProjectId, _: &Namespace) -> Result<()> {
        Ok(())
    }
    async fn list(&self, _: &ProjectId) -> Result<Vec<Namespace>> {
        Ok(vec![])
    }
}

impl ProjectLinkStore for MockStore {
    async fn save(&self, _: &mut ProjectLink) -> Result<()> {
        unimplemented!()
    }
    async fn delete(&self, _: &ProjectLinkId) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_id(&self, _: &ProjectLinkId) -> Result<Option<ProjectLink>> {
        unimplemented!()
    }
    async fn list_by_target(&self, _: &ProjectId) -> Result<Vec<ProjectLink>> {
        unimplemented!()
    }
    async fn find_link(&self, _: &ProjectId, _: &ProjectId) -> Result<Option<ProjectLink>> {
        unimplemented!()
    }
}

impl LockStore for MockStore {
    async fn save(&self, _: &mut ResourceLock) -> Result<()> {
        unimplemented!()
    }
    async fn find(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<Option<ResourceLock>> {
        unimplemented!()
    }
    async fn delete(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_holder(&self, _: &AgentId) -> Result<Vec<ResourceLock>> {
        Ok(vec![])
    }
    async fn delete_expired(&self) -> Result<u64> {
        unimplemented!()
    }
}

impl DocumentStore for MockStore {
    async fn save(&self, _: &mut Document) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_id(&self, _: &DocumentId) -> Result<Option<Document>> {
        unimplemented!()
    }
    async fn find_by_path(
        &self,
        _: &ProjectId,
        _: &Namespace,
        _: &str,
    ) -> Result<Option<Document>> {
        unimplemented!()
    }
    async fn list(&self, _: DocumentFilter) -> Result<Vec<Document>> {
        unimplemented!()
    }
    async fn search(
        &self,
        _: &str,
        _: Option<&[f32]>,
        _: Option<&Namespace>,
        _: usize,
    ) -> Result<Vec<Document>> {
        unimplemented!()
    }
    async fn delete(&self, _: &DocumentId) -> Result<()> {
        unimplemented!()
    }
}

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
    async fn find_by_task(&self, _: &TaskId) -> Result<Vec<ReviewRequest>> {
        Ok(vec![])
    }
}

impl EntryStore for MockStore {
    async fn save(&self, _: &mut Entry) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &EntryId) -> Result<Option<Entry>> {
        unimplemented!()
    }
    async fn find_by_path(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<Option<Entry>> {
        unimplemented!()
    }
    async fn list(&self, _: EntryFilter) -> Result<Vec<Entry>> {
        unimplemented!()
    }
    async fn search(
        &self,
        _: &str,
        _: Option<&[f32]>,
        _: Option<&Namespace>,
        _: usize,
    ) -> Result<Vec<Entry>> {
        unimplemented!()
    }
    async fn delete(&self, _: &EntryId) -> Result<()> {
        unimplemented!()
    }
}

impl SkillStore for MockStore {
    async fn save(&self, _: &mut Skill) -> Result<()> {
        unimplemented!()
    }
    async fn find_by_name(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<Option<Skill>> {
        unimplemented!()
    }
    async fn list(&self, _: SkillFilter) -> Result<Vec<Skill>> {
        unimplemented!()
    }
    async fn delete(&self, _: &ProjectId, _: &Namespace, _: &str) -> Result<()> {
        unimplemented!()
    }
}
