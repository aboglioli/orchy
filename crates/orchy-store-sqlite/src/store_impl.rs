use orchy_core::entities::{
    Agent, ContextSnapshot, CreateMessage, CreateSnapshot, CreateTask, MemoryEntry, MemoryFilter,
    Message, RegisterAgent, Task, TaskFilter, WriteMemory,
};
use orchy_core::error::Result;
use orchy_core::store::{
    AgentStore, ContextStore, MemoryStore, MessageStore, Store, TaskStore,
};
use orchy_core::value_objects::{AgentId, AgentStatus, MessageId, Namespace, TaskId, TaskStatus};

use crate::SqliteBackend;

impl Store for SqliteBackend {
    // --- TaskStore ---

    async fn create_task(&self, task: CreateTask) -> Result<Task> {
        TaskStore::create(self, task).await
    }

    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        TaskStore::get(self, id).await
    }

    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        TaskStore::list(self, filter).await
    }

    async fn claim_task(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        TaskStore::claim(self, id, agent).await
    }

    async fn complete_task(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        TaskStore::complete(self, id, summary).await
    }

    async fn fail_task(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        TaskStore::fail(self, id, reason).await
    }

    async fn release_task(&self, id: &TaskId) -> Result<Task> {
        TaskStore::release(self, id).await
    }

    async fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        TaskStore::update_status(self, id, status).await
    }

    // --- MemoryStore ---

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

    // --- AgentStore ---

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

    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        AgentStore::disconnect(self, id).await
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        AgentStore::find_timed_out(self, timeout_secs).await
    }

    // --- MessageStore ---

    async fn send_message(&self, message: CreateMessage) -> Result<Message> {
        MessageStore::send(self, message).await
    }

    async fn check_messages(
        &self,
        agent: &AgentId,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<Message>> {
        MessageStore::check(self, agent, namespace).await
    }

    async fn mark_messages_read(&self, ids: &[MessageId]) -> Result<()> {
        MessageStore::mark_read(self, ids).await
    }

    // --- ContextStore ---

    async fn save_context(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        ContextStore::save(self, snapshot).await
    }

    async fn load_context(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        ContextStore::load(self, agent).await
    }

    async fn list_contexts(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        ContextStore::list(self, agent, namespace).await
    }

    async fn search_contexts(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        ContextStore::search(self, query, embedding, namespace, agent_id, limit).await
    }
}
