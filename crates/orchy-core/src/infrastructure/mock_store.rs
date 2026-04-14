use std::collections::HashMap;
use std::sync::RwLock;

use crate::agent::{Agent, AgentId, AgentStore};
use crate::error::Result;
use crate::memory::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore};
use crate::message::{Message, MessageId, MessageStatus, MessageStore, MessageTarget};
use crate::namespace::{Namespace, NamespaceStore, ProjectId};
use crate::project::{Project, ProjectStore};
use crate::project_link::{ProjectLink, ProjectLinkId, ProjectLinkStore};
use crate::skill::{Skill, SkillFilter, SkillStore};
use crate::task::{Task, TaskFilter, TaskId, TaskStore};

#[derive(Debug, Default)]
pub struct MockStore {
    agents: RwLock<HashMap<AgentId, Agent>>,
    messages: RwLock<HashMap<MessageId, Message>>,
}

impl TaskStore for MockStore {
    async fn save(&self, _: &Task) -> Result<()> {
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
    async fn save(&self, agent: &Agent) -> Result<()> {
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
    async fn save(&self, message: &Message) -> Result<()> {
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
    async fn save(&self, _: &Project) -> Result<()> {
        Ok(())
    }
    async fn find_by_id(&self, _: &ProjectId) -> Result<Option<Project>> {
        Ok(None)
    }
}

impl MemoryStore for MockStore {
    async fn save(&self, _: &MemoryEntry) -> Result<()> {
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
    async fn save(&self, _: &ContextSnapshot) -> Result<()> {
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
    async fn save(&self, _: &ProjectLink) -> Result<()> {
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

impl SkillStore for MockStore {
    async fn save(&self, _: &Skill) -> Result<()> {
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
