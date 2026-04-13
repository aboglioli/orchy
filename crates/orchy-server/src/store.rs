use orchy_core::agent::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use orchy_core::error::Result;
use orchy_core::memory::{
    ContextSnapshot, ContextStore, CreateSnapshot, MemoryEntry, MemoryFilter, MemoryStore,
    WriteMemory,
};
use orchy_core::message::{CreateMessage, Message, MessageId, MessageStore};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::project::{Project, ProjectStore};
use orchy_core::skill::{Skill, SkillFilter, SkillStore, WriteSkill};
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

pub enum StoreBackend {
    Memory(MemoryBackend),
    Sqlite(SqliteBackend),
    Postgres(PgBackend),
}

macro_rules! delegate_trait {
    ($self:expr, $Trait:ident :: $method:ident ( $($arg:expr),* )) => {
        match $self {
            StoreBackend::Memory(b) => $Trait::$method(b, $($arg),*).await,
            StoreBackend::Sqlite(b) => $Trait::$method(b, $($arg),*).await,
            StoreBackend::Postgres(b) => $Trait::$method(b, $($arg),*).await,
        }
    };
}

impl TaskStore for StoreBackend {
    async fn save(&self, task: &Task) -> Result<()> {
        delegate_trait!(self, TaskStore::save(task))
    }
    async fn get(&self, id: &TaskId) -> Result<Option<Task>> {
        delegate_trait!(self, TaskStore::get(id))
    }
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        delegate_trait!(self, TaskStore::list(filter))
    }
}

impl AgentStore for StoreBackend {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        delegate_trait!(self, AgentStore::register(registration))
    }
    async fn get(&self, id: &AgentId) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::get(id))
    }
    async fn list(&self) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::list())
    }
    async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        delegate_trait!(self, AgentStore::heartbeat(id))
    }
    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        delegate_trait!(self, AgentStore::update_status(id, status))
    }
    async fn update_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        delegate_trait!(self, AgentStore::update_roles(id, roles))
    }
    async fn reconnect(
        &self,
        id: &AgentId,
        roles: Vec<String>,
        description: String,
    ) -> Result<Agent> {
        delegate_trait!(self, AgentStore::reconnect(id, roles, description))
    }
    async fn disconnect(&self, id: &AgentId) -> Result<()> {
        delegate_trait!(self, AgentStore::disconnect(id))
    }
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::find_timed_out(timeout_secs))
    }
}

impl MessageStore for StoreBackend {
    async fn send(&self, message: CreateMessage) -> Result<Message> {
        delegate_trait!(self, MessageStore::send(message))
    }
    async fn check(&self, agent: &AgentId, namespace: &Namespace) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::check(agent, namespace))
    }
    async fn mark_read(&self, ids: &[MessageId]) -> Result<()> {
        delegate_trait!(self, MessageStore::mark_read(ids))
    }
}

impl MemoryStore for StoreBackend {
    async fn write(&self, entry: WriteMemory) -> Result<MemoryEntry> {
        delegate_trait!(self, MemoryStore::write(entry))
    }
    async fn read(&self, namespace: &Namespace, key: &str) -> Result<Option<MemoryEntry>> {
        delegate_trait!(self, MemoryStore::read(namespace, key))
    }
    async fn list(&self, filter: MemoryFilter) -> Result<Vec<MemoryEntry>> {
        delegate_trait!(self, MemoryStore::list(filter))
    }
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        delegate_trait!(
            self,
            MemoryStore::search(query, embedding, namespace, limit)
        )
    }
    async fn delete(&self, namespace: &Namespace, key: &str) -> Result<()> {
        delegate_trait!(self, MemoryStore::delete(namespace, key))
    }
}

impl ContextStore for StoreBackend {
    async fn save(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot> {
        delegate_trait!(self, ContextStore::save(snapshot))
    }
    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        delegate_trait!(self, ContextStore::load(agent))
    }
    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>> {
        delegate_trait!(self, ContextStore::list(agent, namespace))
    }
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>> {
        delegate_trait!(
            self,
            ContextStore::search(query, embedding, namespace, agent_id, limit)
        )
    }
}

impl SkillStore for StoreBackend {
    async fn write(&self, skill: WriteSkill) -> Result<Skill> {
        delegate_trait!(self, SkillStore::write(skill))
    }
    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        delegate_trait!(self, SkillStore::read(namespace, name))
    }
    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        delegate_trait!(self, SkillStore::list(filter))
    }
    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        delegate_trait!(self, SkillStore::delete(namespace, name))
    }
}

impl ProjectStore for StoreBackend {
    async fn save(&self, project: &Project) -> Result<()> {
        delegate_trait!(self, ProjectStore::save(project))
    }
    async fn get(&self, id: &ProjectId) -> Result<Option<Project>> {
        delegate_trait!(self, ProjectStore::get(id))
    }
}
