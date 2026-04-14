use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::document::{Document, DocumentFilter, DocumentId, DocumentStore};
use orchy_core::error::Result;
use orchy_core::memory::{ContextSnapshot, ContextStore, MemoryEntry, MemoryFilter, MemoryStore};
use orchy_core::message::{Message, MessageId, MessageStore};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::project::{Project, ProjectStore};
use orchy_core::project_link::{ProjectLink, ProjectLinkId, ProjectLinkStore};
use orchy_core::resource_lock::{LockStore, ResourceLock};
use orchy_core::skill::{Skill, SkillFilter, SkillStore};
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
    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        delegate_trait!(self, TaskStore::find_by_id(id))
    }
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        delegate_trait!(self, TaskStore::list(filter))
    }
}

impl AgentStore for StoreBackend {
    async fn save(&self, agent: &Agent) -> Result<()> {
        delegate_trait!(self, AgentStore::save(agent))
    }
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::find_by_id(id))
    }
    async fn list(&self) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::list())
    }
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::find_timed_out(timeout_secs))
    }
}

impl MessageStore for StoreBackend {
    async fn save(&self, message: &Message) -> Result<()> {
        delegate_trait!(self, MessageStore::save(message))
    }
    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        delegate_trait!(self, MessageStore::find_by_id(id))
    }
    async fn find_pending(
        &self,
        agent: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::find_pending(agent, project, namespace))
    }
    async fn find_sent(
        &self,
        sender: &AgentId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::find_sent(sender, project, namespace))
    }
    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::find_thread(message_id, limit))
    }
}

impl MemoryStore for StoreBackend {
    async fn save(&self, entry: &MemoryEntry) -> Result<()> {
        delegate_trait!(self, MemoryStore::save(entry))
    }
    async fn find_by_key(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> Result<Option<MemoryEntry>> {
        delegate_trait!(self, MemoryStore::find_by_key(project, namespace, key))
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
    async fn delete(&self, project: &ProjectId, namespace: &Namespace, key: &str) -> Result<()> {
        delegate_trait!(self, MemoryStore::delete(project, namespace, key))
    }
}

impl ContextStore for StoreBackend {
    async fn save(&self, snapshot: &ContextSnapshot) -> Result<()> {
        delegate_trait!(self, ContextStore::save(snapshot))
    }
    async fn find_latest(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>> {
        delegate_trait!(self, ContextStore::find_latest(agent))
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
    async fn save(&self, skill: &Skill) -> Result<()> {
        delegate_trait!(self, SkillStore::save(skill))
    }
    async fn find_by_name(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<Skill>> {
        delegate_trait!(self, SkillStore::find_by_name(project, namespace, name))
    }
    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        delegate_trait!(self, SkillStore::list(filter))
    }
    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        delegate_trait!(self, SkillStore::delete(project, namespace, name))
    }
}

impl ProjectStore for StoreBackend {
    async fn save(&self, project: &Project) -> Result<()> {
        delegate_trait!(self, ProjectStore::save(project))
    }
    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        delegate_trait!(self, ProjectStore::find_by_id(id))
    }
}

impl ProjectLinkStore for StoreBackend {
    async fn save(&self, link: &ProjectLink) -> Result<()> {
        delegate_trait!(self, ProjectLinkStore::save(link))
    }
    async fn delete(&self, id: &ProjectLinkId) -> Result<()> {
        delegate_trait!(self, ProjectLinkStore::delete(id))
    }
    async fn find_by_id(&self, id: &ProjectLinkId) -> Result<Option<ProjectLink>> {
        delegate_trait!(self, ProjectLinkStore::find_by_id(id))
    }
    async fn list_by_target(&self, target: &ProjectId) -> Result<Vec<ProjectLink>> {
        delegate_trait!(self, ProjectLinkStore::list_by_target(target))
    }
    async fn find_link(
        &self,
        source: &ProjectId,
        target: &ProjectId,
    ) -> Result<Option<ProjectLink>> {
        delegate_trait!(self, ProjectLinkStore::find_link(source, target))
    }
}

impl DocumentStore for StoreBackend {
    async fn save(&self, doc: &Document) -> Result<()> {
        delegate_trait!(self, DocumentStore::save(doc))
    }
    async fn find_by_id(&self, id: &DocumentId) -> Result<Option<Document>> {
        delegate_trait!(self, DocumentStore::find_by_id(id))
    }
    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Document>> {
        delegate_trait!(self, DocumentStore::find_by_path(project, namespace, path))
    }
    async fn list(&self, filter: DocumentFilter) -> Result<Vec<Document>> {
        delegate_trait!(self, DocumentStore::list(filter))
    }
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Document>> {
        delegate_trait!(
            self,
            DocumentStore::search(query, embedding, namespace, limit)
        )
    }
    async fn delete(&self, id: &DocumentId) -> Result<()> {
        delegate_trait!(self, DocumentStore::delete(id))
    }
}

impl LockStore for StoreBackend {
    async fn save(&self, lock: &ResourceLock) -> Result<()> {
        delegate_trait!(self, LockStore::save(lock))
    }
    async fn find(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        delegate_trait!(self, LockStore::find(project, namespace, name))
    }
    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        delegate_trait!(self, LockStore::delete(project, namespace, name))
    }
    async fn delete_expired(&self) -> Result<u64> {
        delegate_trait!(self, LockStore::delete_expired())
    }
}

impl NamespaceStore for StoreBackend {
    async fn register(&self, project: &ProjectId, namespace: &Namespace) -> Result<()> {
        delegate_trait!(self, NamespaceStore::register(project, namespace))
    }
    async fn list(&self, project: &ProjectId) -> Result<Vec<Namespace>> {
        delegate_trait!(self, NamespaceStore::list(project))
    }
}
