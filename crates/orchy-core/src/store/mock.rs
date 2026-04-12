use std::collections::HashMap;
use std::sync::RwLock;

use crate::entities::{
    Agent, ContextSnapshot, CreateMessage, CreateSnapshot, CreateTask, MemoryEntry, MemoryFilter,
    Message, RegisterAgent, Skill, SkillFilter, Task, TaskFilter, WriteMemory, WriteSkill,
};
use crate::error::{Error, Result};
use crate::value_objects::{
    AgentId, AgentStatus, MessageId, MessageTarget, Namespace, TaskId, TaskStatus,
};

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
            status: crate::entities::MessageStatus::Pending,
            created_at: chrono::Utc::now(),
        };
        self.messages.write().unwrap().push(msg.clone());
        Ok(msg)
    }
    async fn check_messages(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
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
