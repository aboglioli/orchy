use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orchy_events::Event;
use orchy_events::io::Writer as EventWriter;

use orchy_application::EventQuery;
use orchy_core::agent::{Agent, AgentId, AgentStore, Alias};
use orchy_core::error::Result;
use orchy_core::graph::{Edge, EdgeId, EdgeStore, RelationType, TraversalDirection, TraversalHop};
use orchy_core::knowledge::{
    Knowledge, KnowledgeFilter, KnowledgeId, KnowledgePath, KnowledgeStore,
};
use orchy_core::message::{Message, MessageId, MessageStore};
use orchy_core::namespace::{Namespace, NamespaceStore, ProjectId};
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};
use orchy_core::pagination::{Page, PageParams};
use orchy_core::project::{Project, ProjectStore};
use orchy_core::resource_lock::{LockStore, ResourceLock};
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};
use orchy_core::user::{OrgMembership, OrgMembershipStore, User, UserId, UserStore};
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
            StoreBackend::Memory(b) => b.query_events(organization, since, limit).await,
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

#[async_trait]
impl OrganizationStore for StoreBackend {
    async fn save(&self, org: &mut Organization) -> Result<()> {
        delegate_trait!(self, OrganizationStore::save(org))
    }
    async fn find_by_id(&self, id: &OrganizationId) -> Result<Option<Organization>> {
        delegate_trait!(self, OrganizationStore::find_by_id(id))
    }
    async fn find_by_api_key(&self, key: &str) -> Result<Option<Organization>> {
        delegate_trait!(self, OrganizationStore::find_by_api_key(key))
    }
    async fn list(&self) -> Result<Vec<Organization>> {
        delegate_trait!(self, OrganizationStore::list())
    }
}

#[async_trait]
impl TaskStore for StoreBackend {
    async fn save(&self, task: &mut Task) -> Result<()> {
        delegate_trait!(self, TaskStore::save(task))
    }
    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        delegate_trait!(self, TaskStore::find_by_id(id))
    }
    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        delegate_trait!(self, TaskStore::find_by_ids(ids))
    }
    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        delegate_trait!(self, TaskStore::list(filter, page))
    }
}

#[async_trait]
impl AgentStore for StoreBackend {
    async fn save(&self, agent: &mut Agent) -> Result<()> {
        delegate_trait!(self, AgentStore::save(agent))
    }
    async fn find_by_id(&self, id: &AgentId) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::find_by_id(id))
    }
    async fn find_by_ids(&self, ids: &[AgentId]) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::find_by_ids(ids))
    }
    async fn find_by_alias(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        alias: &Alias,
    ) -> Result<Option<Agent>> {
        delegate_trait!(self, AgentStore::find_by_alias(org, project, alias))
    }
    async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        delegate_trait!(self, AgentStore::list(org, page))
    }
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        delegate_trait!(self, AgentStore::find_timed_out(timeout_secs))
    }
}

#[async_trait]
impl MessageStore for StoreBackend {
    async fn save(&self, message: &mut Message) -> Result<()> {
        delegate_trait!(self, MessageStore::save(message))
    }
    async fn find_by_id(&self, id: &MessageId) -> Result<Option<Message>> {
        delegate_trait!(self, MessageStore::find_by_id(id))
    }
    async fn find_by_ids(&self, ids: &[MessageId]) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::find_by_ids(ids))
    }
    async fn mark_read(&self, agent: &AgentId, message_ids: &[MessageId]) -> Result<()> {
        delegate_trait!(self, MessageStore::mark_read(agent, message_ids))
    }
    async fn find_unread(
        &self,
        agent: &AgentId,
        agent_roles: &[String],
        agent_namespace: &Namespace,
        agent_user_id: Option<&UserId>,
        org: &OrganizationId,
        project: &ProjectId,
        page: PageParams,
    ) -> Result<Page<Message>> {
        delegate_trait!(
            self,
            MessageStore::find_unread(
                agent,
                agent_roles,
                agent_namespace,
                agent_user_id,
                org,
                project,
                page
            )
        )
    }
    async fn find_sent(
        &self,
        sender: &AgentId,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        page: PageParams,
    ) -> Result<Page<Message>> {
        delegate_trait!(
            self,
            MessageStore::find_sent(sender, org, project, namespace, page)
        )
    }
    async fn find_thread(
        &self,
        message_id: &MessageId,
        limit: Option<usize>,
    ) -> Result<Vec<Message>> {
        delegate_trait!(self, MessageStore::find_thread(message_id, limit))
    }
}

#[async_trait]
impl ProjectStore for StoreBackend {
    async fn save(&self, project: &mut Project) -> Result<()> {
        delegate_trait!(self, ProjectStore::save(project))
    }
    async fn find_by_id(&self, org: &OrganizationId, id: &ProjectId) -> Result<Option<Project>> {
        delegate_trait!(self, ProjectStore::find_by_id(org, id))
    }
}

#[async_trait]
impl KnowledgeStore for StoreBackend {
    async fn save(&self, entry: &mut Knowledge) -> Result<()> {
        delegate_trait!(self, KnowledgeStore::save(entry))
    }
    async fn find_by_id(&self, id: &KnowledgeId) -> Result<Option<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::find_by_id(id))
    }
    async fn find_by_ids(&self, ids: &[KnowledgeId]) -> Result<Vec<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::find_by_ids(ids))
    }
    async fn find_by_path(
        &self,
        org: &OrganizationId,
        project: Option<&ProjectId>,
        namespace: &Namespace,
        path: &KnowledgePath,
    ) -> Result<Option<Knowledge>> {
        delegate_trait!(
            self,
            KnowledgeStore::find_by_path(org, project, namespace, path)
        )
    }
    async fn list(&self, filter: KnowledgeFilter, page: PageParams) -> Result<Page<Knowledge>> {
        delegate_trait!(self, KnowledgeStore::list(filter, page))
    }
    async fn search(
        &self,
        org: &OrganizationId,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> Result<Vec<(Knowledge, Option<f32>)>> {
        delegate_trait!(
            self,
            KnowledgeStore::search(org, query, embedding, namespace, limit)
        )
    }
    async fn delete(&self, id: &KnowledgeId) -> Result<()> {
        delegate_trait!(self, KnowledgeStore::delete(id))
    }
}

#[async_trait]
impl LockStore for StoreBackend {
    async fn save(&self, lock: &mut ResourceLock) -> Result<()> {
        delegate_trait!(self, LockStore::save(lock))
    }
    async fn find(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<ResourceLock>> {
        delegate_trait!(self, LockStore::find(org, project, namespace, name))
    }
    async fn delete(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<()> {
        delegate_trait!(self, LockStore::delete(org, project, namespace, name))
    }
    async fn find_by_holder(
        &self,
        holder: &AgentId,
        org: &OrganizationId,
    ) -> Result<Vec<ResourceLock>> {
        delegate_trait!(self, LockStore::find_by_holder(holder, org))
    }
    async fn delete_expired(&self) -> Result<u64> {
        delegate_trait!(self, LockStore::delete_expired())
    }
}

#[async_trait]
impl NamespaceStore for StoreBackend {
    async fn register(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> Result<()> {
        delegate_trait!(self, NamespaceStore::register(org, project, namespace))
    }
    async fn list(&self, org: &OrganizationId, project: &ProjectId) -> Result<Vec<Namespace>> {
        delegate_trait!(self, NamespaceStore::list(org, project))
    }
}

#[async_trait]
impl EdgeStore for StoreBackend {
    async fn save(&self, edge: &mut Edge) -> Result<()> {
        delegate_trait!(self, EdgeStore::save(edge))
    }
    async fn find_by_id(&self, id: &EdgeId) -> Result<Option<Edge>> {
        delegate_trait!(self, EdgeStore::find_by_id(id))
    }
    async fn delete(&self, id: &EdgeId) -> Result<()> {
        delegate_trait!(self, EdgeStore::delete(id))
    }
    async fn find_from(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        delegate_trait!(self, EdgeStore::find_from(org, kind, id, rel_types, as_of))
    }
    async fn find_to(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Vec<Edge>> {
        delegate_trait!(self, EdgeStore::find_to(org, kind, id, rel_types, as_of))
    }
    async fn exists_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<bool> {
        delegate_trait!(
            self,
            EdgeStore::exists_by_pair(org, from_kind, from_id, to_kind, to_id, rel_type)
        )
    }
    async fn find_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<Option<Edge>> {
        delegate_trait!(
            self,
            EdgeStore::find_by_pair(org, from_kind, from_id, to_kind, to_id, rel_type)
        )
    }
    async fn list_by_org(
        &self,
        org: &OrganizationId,
        rel_type: Option<&RelationType>,
        page: PageParams,
        only_active: bool,
        as_of: Option<DateTime<Utc>>,
    ) -> Result<Page<Edge>> {
        delegate_trait!(
            self,
            EdgeStore::list_by_org(org, rel_type, page, only_active, as_of)
        )
    }
    async fn find_neighbors(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
        rel_types: &[RelationType],
        target_kinds: &[ResourceKind],
        direction: TraversalDirection,
        max_depth: u32,
        as_of: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<TraversalHop>> {
        delegate_trait!(
            self,
            EdgeStore::find_neighbors(
                org,
                kind,
                id,
                rel_types,
                target_kinds,
                direction,
                max_depth,
                as_of,
                limit
            )
        )
    }
    async fn delete_all_for(
        &self,
        org: &OrganizationId,
        kind: &ResourceKind,
        id: &str,
    ) -> Result<()> {
        delegate_trait!(self, EdgeStore::delete_all_for(org, kind, id))
    }
    async fn delete_by_pair(
        &self,
        org: &OrganizationId,
        from_kind: &ResourceKind,
        from_id: &str,
        to_kind: &ResourceKind,
        to_id: &str,
        rel_type: &RelationType,
    ) -> Result<()> {
        delegate_trait!(
            self,
            EdgeStore::delete_by_pair(org, from_kind, from_id, to_kind, to_id, rel_type)
        )
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

#[async_trait]
impl EventQuery for StoreBackend {
    async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<orchy_events::SerializedEvent>> {
        match self {
            StoreBackend::Memory(b) => b.query_events(organization, since, limit).await,
            StoreBackend::Sqlite(b) => b.query_events(organization, since, limit),
            StoreBackend::Postgres(b) => b.query_events(organization, since, limit).await,
        }
    }
}

#[async_trait]
impl UserStore for StoreBackend {
    async fn save(&self, user: &mut User) -> Result<()> {
        delegate_trait!(self, UserStore::save(user))
    }
    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        delegate_trait!(self, UserStore::find_by_id(id))
    }
    async fn find_by_email(&self, email: &orchy_core::user::Email) -> Result<Option<User>> {
        delegate_trait!(self, UserStore::find_by_email(email))
    }
    async fn list_all(&self) -> Result<Vec<User>> {
        delegate_trait!(self, UserStore::list_all())
    }
}

#[async_trait]
impl OrgMembershipStore for StoreBackend {
    async fn save(&self, membership: &OrgMembership) -> Result<()> {
        delegate_trait!(self, OrgMembershipStore::save(membership))
    }
    async fn find_by_id(
        &self,
        id: &orchy_core::user::MembershipId,
    ) -> Result<Option<OrgMembership>> {
        delegate_trait!(self, OrgMembershipStore::find_by_id(id))
    }
    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        delegate_trait!(self, OrgMembershipStore::find_by_user(user_id))
    }
    async fn find_by_org(
        &self,
        org_id: &orchy_core::organization::OrganizationId,
    ) -> Result<Vec<OrgMembership>> {
        delegate_trait!(self, OrgMembershipStore::find_by_org(org_id))
    }
    async fn find(
        &self,
        user_id: &UserId,
        org_id: &orchy_core::organization::OrganizationId,
    ) -> Result<Option<OrgMembership>> {
        delegate_trait!(self, OrgMembershipStore::find(user_id, org_id))
    }
    async fn delete(&self, id: &orchy_core::user::MembershipId) -> Result<()> {
        delegate_trait!(self, OrgMembershipStore::delete(id))
    }
}
