use async_trait::async_trait;
use orchy_events::Event;
use orchy_events::io::Writer as EventWriter;

use orchy_core::agent::{Agent, AgentId, AgentStore};
use orchy_core::error::Result;
use orchy_core::knowledge::{Knowledge, KnowledgeFilter, KnowledgeId, KnowledgeStore};
use orchy_core::message::{Message, MessageId, MessageStore};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::project::{Project, ProjectStore};
use orchy_core::project_link::{ProjectLink, ProjectLinkId, ProjectLinkStore};
use orchy_core::resource_lock::{LockStore, ResourceLock};
use orchy_core::task::{
    ReviewId, ReviewRequest, ReviewStore, Task, TaskFilter, TaskId, TaskStore, TaskWatcher,
    WatcherStore,
};
use orchy_store_memory::MemoryBackend;
use orchy_store_pg::PgBackend;
use orchy_store_sqlite::SqliteBackend;

#[allow(clippy::large_enum_variant)]
pub enum StoreBackend {
    Memory(MemoryBackend),
    Sqlite(SqliteBackend),
    Postgres(PgBackend),
}

impl StoreBackend {
    pub async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> orchy_core::error::Result<Vec<orchy_events::SerializedEvent>> {
        match self {
            StoreBackend::Memory(b) => {
                let events = b.list_events()?;
                let mut filtered: Vec<_> = events
                    .into_iter()
                    .filter(|e| e.organization == organization && e.timestamp >= since)
                    .collect();
                filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
                filtered.truncate(limit);
                filtered.reverse();
                Ok(filtered)
            }
            StoreBackend::Sqlite(b) => b.query_events(organization, since, limit),
            StoreBackend::Postgres(b) => b.query_events(organization, since, limit).await,
        }
    }
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
    async fn save(&self, task: &mut Task) -> Result<()> {
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
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        delegate_trait!(self, AgentStore::save(agent))
    }
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::find_by_id(id))
    }
    async fn find_by_alias(
        &self,
        project: &ProjectId,
        alias: &orchy_core::agent::Alias,
    ) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::find_by_alias(project, alias))
    }
    async fn list(&self) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::list())
    }
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::find_timed_out(timeout_secs))
    }
}

impl MessageStore for StoreBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
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

impl ProjectStore for StoreBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        delegate_trait!(self, ProjectStore::save(project))
    }
    async fn find_by_id(&self, id: &ProjectId) -> Result<Option<Project>> {
        delegate_trait!(self, ProjectStore::find_by_id(id))
    }
}

impl ProjectLinkStore for StoreBackend {
    async fn save(&self, link: &mut ProjectLink) -> Result<()> {
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

impl KnowledgeStore for StoreBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        delegate_trait!(self, KnowledgeStore::save(entry))
    }
    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::find_by_id(id))
    }
    async fn find_by_path(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        path: &str,
    ) -> Result<Option<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::find_by_path(project, namespace, path))
    }
    async fn list(&self, filter: KnowledgeFilter) -> Result<Vec<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::list(filter))
    }
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<Knowledge>> {
        delegate_trait!(
            self,
            KnowledgeStore::search(query, embedding, namespace, limit)
        )
    }
    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        delegate_trait!(self, KnowledgeStore::delete(id))
    }
}

impl LockStore for StoreBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
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
    async fn find_by_holder(&self, holder: &AgentId) -> Result<Vec<ResourceLock>> {
        delegate_trait!(self, LockStore::find_by_holder(holder))
    }
    async fn delete_expired(&self) -> Result<u64> {
        delegate_trait!(self, LockStore::delete_expired())
    }
}

impl WatcherStore for StoreBackend {
    async fn save(&self, watcher: &mut TaskWatcher) -> Result<()> {
        delegate_trait!(self, WatcherStore::save(watcher))
    }
    async fn delete(&self, task_id: &TaskId, agent_id: &AgentId) -> Result<()> {
        delegate_trait!(self, WatcherStore::delete(task_id, agent_id))
    }
    async fn find_watchers(&self, task_id: &TaskId) -> Result<Vec<TaskWatcher>> {
        delegate_trait!(self, WatcherStore::find_watchers(task_id))
    }
    async fn find_by_agent(&self, agent_id: &AgentId) -> Result<Vec<TaskWatcher>> {
        delegate_trait!(self, WatcherStore::find_by_agent(agent_id))
    }
}

impl ReviewStore for StoreBackend {
    async fn save(&self, review: &mut ReviewRequest) -> Result<()> {
        delegate_trait!(self, ReviewStore::save(review))
    }
    async fn find_by_id(&self, id: &ReviewId) -> Result<Option<ReviewRequest>> {
        delegate_trait!(self, ReviewStore::find_by_id(id))
    }
    async fn find_pending_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        delegate_trait!(self, ReviewStore::find_pending_for_agent(agent_id))
    }
    async fn find_by_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        delegate_trait!(self, ReviewStore::find_by_task(task_id))
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

#[async_trait]
impl EventWriter for StoreBackend {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        match self {
            StoreBackend::Memory(b) => EventWriter::write(b, event).await,
            StoreBackend::Sqlite(b) => EventWriter::write(b, event).await,
            StoreBackend::Postgres(b) => EventWriter::write(b, event).await,
        }
    }
}
