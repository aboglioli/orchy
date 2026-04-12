use std::future::Future;

use crate::agent::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use crate::error::Result;
use crate::memory::{
    ContextSnapshot, ContextStore, CreateSnapshot, MemoryEntry, MemoryFilter, MemoryStore,
    WriteMemory,
};
use crate::message::{CreateMessage, Message, MessageId, MessageStore};
use crate::namespace::Namespace;
use crate::skill::{Skill, SkillFilter, SkillStore, WriteSkill};
use crate::task::{CreateTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

pub trait Store: Send + Sync {
    fn create_task(&self, task: CreateTask) -> impl Future<Output = Result<Task>> + Send;
    fn get_task(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list_tasks(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
    fn claim_task(&self, id: &TaskId, agent: &AgentId)
    -> impl Future<Output = Result<Task>> + Send;
    fn complete_task(
        &self,
        id: &TaskId,
        summary: Option<String>,
    ) -> impl Future<Output = Result<Task>> + Send;
    fn fail_task(
        &self,
        id: &TaskId,
        reason: Option<String>,
    ) -> impl Future<Output = Result<Task>> + Send;
    fn release_task(&self, id: &TaskId) -> impl Future<Output = Result<Task>> + Send;
    fn update_task(&self, task: &Task) -> impl Future<Output = Result<Task>> + Send;
    fn update_task_status(
        &self,
        id: &TaskId,
        status: TaskStatus,
    ) -> impl Future<Output = Result<()>> + Send;

    fn write_memory(&self, entry: WriteMemory) -> impl Future<Output = Result<MemoryEntry>> + Send;
    fn read_memory(
        &self,
        namespace: &Namespace,
        key: &str,
    ) -> impl Future<Output = Result<Option<MemoryEntry>>> + Send;
    fn list_memory(
        &self,
        filter: MemoryFilter,
    ) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn search_memory(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn delete_memory(
        &self,
        namespace: &Namespace,
        key: &str,
    ) -> impl Future<Output = Result<()>> + Send;

    fn register(&self, registration: RegisterAgent) -> impl Future<Output = Result<Agent>> + Send;
    fn get_agent(&self, id: &AgentId) -> impl Future<Output = Result<Option<Agent>>> + Send;
    fn list_agents(&self) -> impl Future<Output = Result<Vec<Agent>>> + Send;
    fn heartbeat(&self, id: &AgentId) -> impl Future<Output = Result<()>> + Send;
    fn update_agent_status(
        &self,
        id: &AgentId,
        status: AgentStatus,
    ) -> impl Future<Output = Result<()>> + Send;
    fn update_agent_roles(
        &self,
        id: &AgentId,
        roles: Vec<String>,
    ) -> impl Future<Output = Result<Agent>> + Send;
    fn disconnect(&self, id: &AgentId) -> impl Future<Output = Result<()>> + Send;
    fn find_timed_out(&self, timeout_secs: u64) -> impl Future<Output = Result<Vec<Agent>>> + Send;

    fn send_message(&self, message: CreateMessage) -> impl Future<Output = Result<Message>> + Send;
    fn check_messages(
        &self,
        agent: &AgentId,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<Message>>> + Send;
    fn mark_messages_read(&self, ids: &[MessageId]) -> impl Future<Output = Result<()>> + Send;

    fn save_context(
        &self,
        snapshot: CreateSnapshot,
    ) -> impl Future<Output = Result<ContextSnapshot>> + Send;
    fn load_context(
        &self,
        agent: &AgentId,
    ) -> impl Future<Output = Result<Option<ContextSnapshot>>> + Send;
    fn list_contexts(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<ContextSnapshot>>> + Send;
    fn search_contexts(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ContextSnapshot>>> + Send;

    fn write_skill(&self, skill: WriteSkill) -> impl Future<Output = Result<Skill>> + Send;
    fn read_skill(
        &self,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<Option<Skill>>> + Send;
    fn list_skills(&self, filter: SkillFilter) -> impl Future<Output = Result<Vec<Skill>>> + Send;
    fn delete_skill(
        &self,
        namespace: &Namespace,
        name: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

#[cfg(test)]
pub mod mock {
    use std::collections::HashMap;
    use std::sync::RwLock;

    use crate::agent::{Agent, AgentId, AgentStatus, RegisterAgent};
    use crate::error::{Error, Result};
    use crate::memory::{ContextSnapshot, CreateSnapshot, MemoryEntry, MemoryFilter, WriteMemory};
    use crate::message::{CreateMessage, Message, MessageId, MessageStatus, MessageTarget};
    use crate::namespace::Namespace;
    use crate::skill::{Skill, SkillFilter, WriteSkill};
    use crate::task::{CreateTask, Task, TaskFilter, TaskId, TaskStatus};

    use super::Store;

    #[derive(Debug, Default)]
    pub struct MockStore {
        agents: RwLock<HashMap<AgentId, Agent>>,
        messages: RwLock<Vec<Message>>,
    }

    impl Store for MockStore {
        async fn create_task(&self, _: CreateTask) -> Result<Task> {
            unimplemented!()
        }
        async fn get_task(&self, _: &TaskId) -> Result<Option<Task>> {
            unimplemented!()
        }
        async fn list_tasks(&self, _: TaskFilter) -> Result<Vec<Task>> {
            unimplemented!()
        }
        async fn claim_task(&self, _: &TaskId, _: &AgentId) -> Result<Task> {
            unimplemented!()
        }
        async fn complete_task(&self, _: &TaskId, _: Option<String>) -> Result<Task> {
            unimplemented!()
        }
        async fn fail_task(&self, _: &TaskId, _: Option<String>) -> Result<Task> {
            unimplemented!()
        }
        async fn release_task(&self, _: &TaskId) -> Result<Task> {
            unimplemented!()
        }
        async fn update_task(&self, _: &Task) -> Result<Task> {
            unimplemented!()
        }
        async fn update_task_status(&self, _: &TaskId, _: TaskStatus) -> Result<()> {
            unimplemented!()
        }

        async fn register(&self, reg: RegisterAgent) -> Result<Agent> {
            let agent = Agent {
                id: AgentId::new(),
                namespace: reg.namespace,
                roles: reg.roles,
                description: reg.description,
                status: AgentStatus::Online,
                last_heartbeat: chrono::Utc::now(),
                connected_at: chrono::Utc::now(),
                metadata: reg.metadata,
            };
            self.agents.write().unwrap().insert(agent.id, agent.clone());
            Ok(agent)
        }
        async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>> {
            Ok(self.agents.read().unwrap().get(id).cloned())
        }
        async fn list_agents(&self) -> Result<Vec<Agent>> {
            Ok(self.agents.read().unwrap().values().cloned().collect())
        }
        async fn heartbeat(&self, _: &AgentId) -> Result<()> {
            Ok(())
        }
        async fn update_agent_status(&self, _: &AgentId, _: AgentStatus) -> Result<()> {
            Ok(())
        }
        async fn update_agent_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
            let mut agents = self.agents.write().unwrap();
            let agent = agents
                .get_mut(id)
                .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;
            agent.roles = roles;
            Ok(agent.clone())
        }
        async fn disconnect(&self, _: &AgentId) -> Result<()> {
            Ok(())
        }
        async fn find_timed_out(&self, _: u64) -> Result<Vec<Agent>> {
            Ok(vec![])
        }

        async fn send_message(&self, cmd: CreateMessage) -> Result<Message> {
            let msg = Message {
                id: MessageId::new(),
                namespace: cmd.namespace,
                from: cmd.from,
                to: cmd.to,
                body: cmd.body,
                status: MessageStatus::Pending,
                created_at: chrono::Utc::now(),
            };
            self.messages.write().unwrap().push(msg.clone());
            Ok(msg)
        }
        async fn check_messages(
            &self,
            agent: &AgentId,
            namespace: &Namespace,
        ) -> Result<Vec<Message>> {
            Ok(self
                .messages
                .read()
                .unwrap()
                .iter()
                .filter(|m| m.to == MessageTarget::Agent(*agent) && m.namespace == *namespace)
                .cloned()
                .collect())
        }
        async fn mark_messages_read(&self, _: &[MessageId]) -> Result<()> {
            Ok(())
        }

        async fn save_context(&self, _: CreateSnapshot) -> Result<ContextSnapshot> {
            unimplemented!()
        }
        async fn load_context(&self, _: &AgentId) -> Result<Option<ContextSnapshot>> {
            unimplemented!()
        }
        async fn list_contexts(
            &self,
            _: Option<&AgentId>,
            _: &Namespace,
        ) -> Result<Vec<ContextSnapshot>> {
            unimplemented!()
        }
        async fn search_contexts(
            &self,
            _: &str,
            _: Option<&[f32]>,
            _: &Namespace,
            _: Option<&AgentId>,
            _: usize,
        ) -> Result<Vec<ContextSnapshot>> {
            unimplemented!()
        }

        async fn write_skill(&self, _: WriteSkill) -> Result<Skill> {
            unimplemented!()
        }
        async fn read_skill(&self, _: &Namespace, _: &str) -> Result<Option<Skill>> {
            unimplemented!()
        }
        async fn list_skills(&self, _: SkillFilter) -> Result<Vec<Skill>> {
            unimplemented!()
        }
        async fn delete_skill(&self, _: &Namespace, _: &str) -> Result<()> {
            unimplemented!()
        }

        async fn write_memory(&self, _: WriteMemory) -> Result<MemoryEntry> {
            unimplemented!()
        }
        async fn read_memory(&self, _: &Namespace, _: &str) -> Result<Option<MemoryEntry>> {
            unimplemented!()
        }
        async fn list_memory(&self, _: MemoryFilter) -> Result<Vec<MemoryEntry>> {
            unimplemented!()
        }
        async fn search_memory(
            &self,
            _: &str,
            _: Option<&[f32]>,
            _: Option<&Namespace>,
            _: usize,
        ) -> Result<Vec<MemoryEntry>> {
            unimplemented!()
        }
        async fn delete_memory(&self, _: &Namespace, _: &str) -> Result<()> {
            unimplemented!()
        }
    }
}
