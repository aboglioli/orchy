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

use std::future::Future;

use crate::entities::{
    Agent, ContextSnapshot, CreateMessage, CreateSnapshot, CreateTask, MemoryEntry, MemoryFilter,
    Message, RegisterAgent, Task, TaskFilter, WriteMemory,
};
use crate::error::Result;
use crate::value_objects::{AgentId, AgentStatus, MessageId, Namespace, TaskId, TaskStatus};

/// Combined storage interface used by services.
/// Concrete backends implement this via a dispatch enum in the server crate.
pub trait Store: Send + Sync {
    // --- TaskStore ---
    fn create_task(&self, task: CreateTask) -> impl Future<Output = Result<Task>> + Send;
    fn get_task(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list_tasks(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
    fn claim_task(&self, id: &TaskId, agent: &AgentId) -> impl Future<Output = Result<Task>> + Send;
    fn complete_task(&self, id: &TaskId, summary: Option<String>) -> impl Future<Output = Result<Task>> + Send;
    fn fail_task(&self, id: &TaskId, reason: Option<String>) -> impl Future<Output = Result<Task>> + Send;
    fn release_task(&self, id: &TaskId) -> impl Future<Output = Result<Task>> + Send;
    fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> impl Future<Output = Result<()>> + Send;

    // --- MemoryStore ---
    fn write_memory(&self, entry: WriteMemory) -> impl Future<Output = Result<MemoryEntry>> + Send;
    fn read_memory(&self, namespace: &Namespace, key: &str) -> impl Future<Output = Result<Option<MemoryEntry>>> + Send;
    fn list_memory(&self, filter: MemoryFilter) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn search_memory(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn delete_memory(&self, namespace: &Namespace, key: &str) -> impl Future<Output = Result<()>> + Send;

    // --- AgentStore ---
    fn register(&self, registration: RegisterAgent) -> impl Future<Output = Result<Agent>> + Send;
    fn get_agent(&self, id: &AgentId) -> impl Future<Output = Result<Option<Agent>>> + Send;
    fn list_agents(&self) -> impl Future<Output = Result<Vec<Agent>>> + Send;
    fn heartbeat(&self, id: &AgentId) -> impl Future<Output = Result<()>> + Send;
    fn update_agent_status(&self, id: &AgentId, status: AgentStatus) -> impl Future<Output = Result<()>> + Send;
    fn disconnect(&self, id: &AgentId) -> impl Future<Output = Result<()>> + Send;
    fn find_timed_out(&self, timeout_secs: u64) -> impl Future<Output = Result<Vec<Agent>>> + Send;

    // --- MessageStore ---
    fn send_message(&self, message: CreateMessage) -> impl Future<Output = Result<Message>> + Send;
    fn check_messages(&self, agent: &AgentId, namespace: &Namespace) -> impl Future<Output = Result<Vec<Message>>> + Send;
    fn mark_messages_read(&self, ids: &[MessageId]) -> impl Future<Output = Result<()>> + Send;

    // --- ContextStore ---
    fn save_context(&self, snapshot: CreateSnapshot) -> impl Future<Output = Result<ContextSnapshot>> + Send;
    fn load_context(&self, agent: &AgentId) -> impl Future<Output = Result<Option<ContextSnapshot>>> + Send;
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
}
