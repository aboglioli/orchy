pub mod agent_store;
pub mod context_store;
pub mod memory_store;
pub mod message_store;
pub mod task_store;

pub use agent_store::AgentStore;
pub use context_store::ContextStore;
pub use memory_store::MemoryStore;
pub use message_store::MessageStore;
pub use task_store::TaskStore;

use crate::entities::{
    Agent, ContextSnapshot, CreateMessage, CreateSnapshot, CreateTask, MemoryEntry, MemoryFilter,
    Message, RegisterAgent, Task, TaskFilter, WriteMemory,
};
use crate::error::Result;
use crate::value_objects::{AgentId, AgentStatus, MessageId, Namespace, TaskId, TaskStatus};

/// Enum dispatch for storage backends. Resolved once at startup.
/// Variants and delegation methods added as backends are implemented.
pub enum Store {
    // Memory(orchy_store_memory::MemoryBackend),
    // Sqlite(orchy_store_sqlite::SqliteBackend),
    // Postgres(orchy_store_pg::PgBackend),
}

impl Store {
    // --- TaskStore ---

    pub async fn create_task(&self, task: CreateTask) -> Result<Task> {
        match *self {}
    }

    pub async fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        match *self {}
    }

    pub async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        match *self {}
    }

    pub async fn claim_task(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        match *self {}
    }

    pub async fn complete_task(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        match *self {}
    }

    pub async fn fail_task(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        match *self {}
    }

    pub async fn release_task(&self, id: &TaskId) -> Result<Task> {
        match *self {}
    }

    pub async fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        match *self {}
    }

    // --- MemoryStore ---

    pub async fn write_memory(&self, entry: WriteMemory) -> Result<MemoryEntry> {
        match *self {}
    }

    pub async fn read_memory(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        match *self {}
    }

    pub async fn list_memory(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        match *self {}
    }

    pub async fn search_memory(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        match *self {}
    }

    pub async fn delete_memory(&self, namespace: &Namespace, key: &str) -> Result<()> {
        match *self {}
    }

    // --- AgentStore ---

    pub async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        match *self {}
    }

    pub async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>> {
        match *self {}
    }

    pub async fn list_agents(&self) -> Result<Vec<Agent>> {
        match *self {}
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        match *self {}
    }

    pub async fn update_agent_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        match *self {}
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        match *self {}
    }

    pub async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        match *self {}
    }

    // --- MessageStore ---

    pub async fn send_message(&self, message: CreateMessage) -> Result<Message> {
        match *self {}
    }

    pub async fn check_messages(
        &self,
        agent: &AgentId,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<Message>> {
        match *self {}
    }

    pub async fn mark_messages_read(&self, ids: &[MessageId]) -> Result<()> {
        match *self {}
    }

    // --- ContextStore ---

    pub async fn save_context(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        match *self {}
    }

    pub async fn load_context(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        match *self {}
    }

    pub async fn list_contexts(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        match *self {}
    }

    pub async fn search_contexts(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        match *self {}
    }
}
