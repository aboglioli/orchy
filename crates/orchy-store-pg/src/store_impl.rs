use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use orchy_core::error::Result;
use orchy_core::memory::{
    ContextSnapshot, ContextStore, CreateSnapshot, MemoryEntry, MemoryFilter, MemoryStore,
    WriteMemory,
};
use orchy_core::message::{CreateMessage, Message, MessageId, MessageStore};
use orchy_core::namespace::Namespace;
use orchy_core::skill::{Skill, SkillFilter, SkillStore, WriteSkill};
use orchy_core::store::Store;
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};

use crate::PgBackend;

impl Store for PgBackend {
    async fn save_task(&self, task: &Task) -> Result<()> {
        TaskStore::save(self, task).await
    }

    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        TaskStore::get(self, id).await
    }

    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        TaskStore::list(self, filter).await
    }

    async fn write_memory(&self, entry: WriteMemory) -> Result<MemoryEntry> {
        MemoryStore::write(self, entry).await
    }

    async fn read_memory(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        MemoryStore::read(self, namespace, key).await
    }

    async fn list_memory(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        MemoryStore::list(self, filter).await
    }

    async fn search_memory(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        MemoryStore::search(self, query, embedding, namespace, limit).await
    }

    async fn delete_memory(&self, namespace: &Namespace, key: &str) -> Result<()> {
        MemoryStore::delete(self, namespace, key).await
    }

    async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        AgentStore::register(self, registration).await
    }

    async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>> {
        AgentStore::get(self, id).await
    }

    async fn list_agents(&self) -> Result<Vec<Agent>> {
        AgentStore::list(self).await
    }

    async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        AgentStore::heartbeat(self, id).await
    }

    async fn update_agent_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        AgentStore::update_status(self, id, status).await
    }

    async fn update_agent_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        AgentStore::update_roles(self, id, roles).await
    }

    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        AgentStore::disconnect(self, id).await
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        AgentStore::find_timed_out(self, timeout_secs).await
    }

    async fn send_message(&self, message: CreateMessage) -> Result<Message> {
        MessageStore::send(self, message).await
    }

    async fn check_messages(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        MessageStore::check(self, agent, namespace).await
    }

    async fn mark_messages_read(&self, ids: &[MessageId]) -> Result<()> {
        MessageStore::mark_read(self, ids).await
    }

    async fn save_context(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        ContextStore::save(self, snapshot).await
    }

    async fn load_context(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        ContextStore::load(self, agent).await
    }

    async fn list_contexts(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        ContextStore::list(self, agent, namespace).await
    }

    async fn search_contexts(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        ContextStore::search(self, query, embedding, namespace, agent_id, limit).await
    }

    async fn write_skill(&self, skill: WriteSkill) -> Result<Skill> {
        SkillStore::write(self, skill).await
    }

    async fn read_skill(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        SkillStore::read(self, namespace, name).await
    }

    async fn list_skills(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        SkillStore::list(self, filter).await
    }

    async fn delete_skill(&self, namespace: &Namespace, name: &str) -> Result<()> {
        SkillStore::delete(self, namespace, name).await
    }
}
