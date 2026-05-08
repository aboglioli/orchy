#![allow(clippy::collapsible_if)]

mod agent;
mod api_key;
mod edge;
mod events;
mod knowledge;
mod membership;
mod message;
mod namespace;
mod organization;
mod project;
mod reader;
mod reader_factory;
mod resource_lock;
mod task;
mod user;

use std::sync::Arc;

use dashmap::{DashMap, DashSet};
use tokio::sync::{Notify, RwLock};

use orchy_events::{Event, SerializedEvent};

use orchy_core::agent::{Agent, AgentId};
use orchy_core::api_key::{ApiKey, ApiKeyId};
use orchy_core::graph::{Edge, EdgeId};
use orchy_core::knowledge::{Knowledge, KnowledgeId};
use orchy_core::message::{Message, MessageId};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::{Organization, OrganizationId};
use orchy_core::pagination::{Page, PageParams};
use orchy_core::project::Project;
use orchy_core::resource_lock::ResourceLock;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{Task, TaskId};
use orchy_core::user::{MembershipId, OrgMembership, User, UserId};

pub use agent::MemoryAgentStore;
pub use api_key::MemoryApiKeyStore;
pub use edge::MemoryEdgeStore;
pub use events::MemoryEventWriter;
pub use knowledge::MemoryKnowledgeStore;
pub use membership::MemoryOrgMembershipStore;
pub use message::MemoryMessageStore;
pub use namespace::MemoryNamespaceStore;
pub use organization::MemoryOrganizationStore;
pub use project::MemoryProjectStore;
pub use reader::{MemoryAcker, MemoryAckerVariant, MemoryReader, MemoryReaderConfig, MemoryStream};
pub use reader_factory::MemoryReaderFactory;
pub use resource_lock::MemoryLockStore;
pub use task::MemoryTaskStore;
pub use user::MemoryUserStore;

pub struct MemoryState {
    pub(crate) agents: DashMap<AgentId, Agent>,
    pub(crate) edges: DashMap<EdgeId, Edge>,
    pub(crate) edges_by_from: DashMap<(ResourceKind, String), Vec<EdgeId>>,
    pub(crate) edges_by_to: DashMap<(ResourceKind, String), Vec<EdgeId>>,
    pub(crate) tasks: DashMap<TaskId, Task>,
    pub(crate) messages: DashMap<MessageId, Message>,
    pub(crate) message_replies: DashMap<MessageId, Vec<MessageId>>,
    pub(crate) message_receipts: DashSet<(MessageId, AgentId)>,
    pub(crate) projects: DashMap<ProjectId, Project>,
    pub(crate) knowledge_entries: DashMap<KnowledgeId, Knowledge>,
    pub(crate) resource_locks: DashMap<String, ResourceLock>,
    pub(crate) namespaces: DashSet<(String, String, String)>,
    pub(crate) api_keys: DashMap<ApiKeyId, ApiKey>,
    pub(crate) organizations: DashMap<OrganizationId, Organization>,
    pub(crate) users: DashMap<UserId, User>,
    pub(crate) user_by_email: DashMap<String, UserId>,
    pub(crate) memberships: DashMap<MembershipId, OrgMembership>,
    pub(crate) memberships_by_user: DashMap<UserId, Vec<MembershipId>>,
    pub(crate) memberships_by_org: DashMap<OrganizationId, Vec<MembershipId>>,
    pub(crate) events: RwLock<Vec<SerializedEvent>>,
    pub(crate) events_notify: Arc<Notify>,
}

impl MemoryState {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
            edges: DashMap::new(),
            edges_by_from: DashMap::new(),
            edges_by_to: DashMap::new(),
            tasks: DashMap::new(),
            messages: DashMap::new(),
            message_replies: DashMap::new(),
            message_receipts: DashSet::new(),
            projects: DashMap::new(),
            knowledge_entries: DashMap::new(),
            resource_locks: DashMap::new(),
            namespaces: DashSet::new(),
            api_keys: DashMap::new(),
            organizations: DashMap::new(),
            users: DashMap::new(),
            user_by_email: DashMap::new(),
            memberships: DashMap::new(),
            memberships_by_user: DashMap::new(),
            memberships_by_org: DashMap::new(),
            events: RwLock::new(Vec::new()),
            events_notify: Arc::new(Notify::new()),
        }
    }
}

impl Default for MemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryState {
    pub async fn snapshot_events(&self) -> Vec<SerializedEvent> {
        self.events.read().await.clone()
    }

    pub fn insert_task(&self, task: Task) {
        self.tasks.insert(task.id(), task);
    }

    pub(crate) async fn append_events(&self, events: Vec<Event>) -> orchy_core::error::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut serialized = Vec::with_capacity(events.len());
        for event in events {
            serialized.push(SerializedEvent::from_event(&event)?);
        }
        self.events.write().await.extend(serialized);
        self.events_notify.notify_waiters();
        Ok(())
    }
}

pub(crate) fn apply_cursor_pagination<T, F>(items: Vec<T>, page: &PageParams, id_fn: F) -> Page<T>
where
    T: serde::Serialize + Clone,
    F: Fn(&T) -> String,
{
    use orchy_core::pagination::{Page, decode_cursor, encode_cursor};

    let start = if let Some(ref cursor) = page.after {
        if let Some(decoded) = decode_cursor(cursor) {
            items
                .iter()
                .position(|i| id_fn(i) == decoded)
                .map(|pos| pos + 1)
                .unwrap_or(0)
        } else {
            0
        }
    } else {
        0
    };

    let limit = page.limit as usize;
    let remaining = items.len().saturating_sub(start);
    let has_more = remaining > limit;
    let result: Vec<T> = items.into_iter().skip(start).take(limit).collect();

    let next_cursor = if has_more {
        result.last().map(|last| encode_cursor(&id_fn(last)))
    } else {
        None
    };

    Page::new(result, next_cursor)
}

pub(crate) fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        0.0
    } else {
        dot / (mag_a * mag_b)
    }
}
