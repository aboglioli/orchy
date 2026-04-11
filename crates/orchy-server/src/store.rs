use orchy_core::entities::{
    Agent, ContextSnapshot, CreateMessage, CreateSnapshot, CreateTask, MemoryEntry, MemoryFilter,
    Message, RegisterAgent, Task, TaskFilter, WriteMemory,
};
use orchy_core::error::Result;
use orchy_core::store::Store;
use orchy_core::value_objects::{AgentId, AgentStatus, MessageId, Namespace, TaskId, TaskStatus};
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

pub enum StoreBackend {
    Memory(MemoryBackend),
    Sqlite(SqliteBackend),
    Postgres(PgBackend),
}

macro_rules! delegate {
    ($self:expr, $method:ident ( $($arg:expr),* )) => {
        match $self {
            StoreBackend::Memory(b) => b.$method($($arg),*).await,
            StoreBackend::Sqlite(b) => b.$method($($arg),*).await,
            StoreBackend::Postgres(b) => b.$method($($arg),*).await,
        }
    };
}

impl Store for StoreBackend {
    // --- TaskStore ---

    async fn create_task(&self, task: CreateTask) -> Result<Task> {
        delegate!(self, create_task(task))
    }

    async fn get_task(&self, id: &TaskId) -> Result<Option<Task>> {
        delegate!(self, get_task(id))
    }

    async fn list_tasks(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        delegate!(self, list_tasks(filter))
    }

    async fn claim_task(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        delegate!(self, claim_task(id, agent))
    }

    async fn complete_task(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        delegate!(self, complete_task(id, summary))
    }

    async fn fail_task(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        delegate!(self, fail_task(id, reason))
    }

    async fn release_task(&self, id: &TaskId) -> Result<Task> {
        delegate!(self, release_task(id))
    }

    async fn update_task_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        delegate!(self, update_task_status(id, status))
    }

    // --- MemoryStore ---

    async fn write_memory(&self, entry: WriteMemory) -> Result<MemoryEntry> {
        delegate!(self, write_memory(entry))
    }

    async fn read_memory(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        delegate!(self, read_memory(namespace, key))
    }

    async fn list_memory(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        delegate!(self, list_memory(filter))
    }

    async fn search_memory(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        delegate!(self, search_memory(query, embedding, namespace, limit))
    }

    async fn delete_memory(&self, namespace: &Namespace, key: &str) -> Result<()> {
        delegate!(self, delete_memory(namespace, key))
    }

    // --- AgentStore ---

    async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        delegate!(self, register(registration))
    }

    async fn get_agent(&self, id: &AgentId) -> Result<Option<Agent>> {
        delegate!(self, get_agent(id))
    }

    async fn list_agents(&self) -> Result<Vec<Agent>> {
        delegate!(self, list_agents())
    }

    async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        delegate!(self, heartbeat(id))
    }

    async fn update_agent_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        delegate!(self, update_agent_status(id, status))
    }

    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        delegate!(self, disconnect(id))
    }

    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        delegate!(self, find_timed_out(timeout_secs))
    }

    // --- MessageStore ---

    async fn send_message(&self, message: CreateMessage) -> Result<Message> {
        delegate!(self, send_message(message))
    }

    async fn check_messages(
        &self,
        agent: &AgentId,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<Message>> {
        delegate!(self, check_messages(agent, namespace))
    }

    async fn mark_messages_read(&self, ids: &[MessageId]) -> Result<()> {
        delegate!(self, mark_messages_read(ids))
    }

    // --- ContextStore ---

    async fn save_context(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        delegate!(self, save_context(snapshot))
    }

    async fn load_context(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        delegate!(self, load_context(agent))
    }

    async fn list_contexts(
        &self,
        agent: Option<&AgentId>,
        namespace: Option<&Namespace>,
    ) -> Result<Vec<ContextSnapshot>> {
        delegate!(self, list_contexts(agent, namespace))
    }

    async fn search_contexts(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        delegate!(self, search_contexts(query, embedding, namespace, agent_id, limit))
    }
}
