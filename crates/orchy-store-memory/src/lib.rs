#![allow(clippy::collapsible_if)]

mod agent;
mod events;
mod knowledge;
mod message;
mod namespace;
mod organization;
mod project;
mod resource_lock;
mod review;
mod task;
mod watcher;

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use orchy_events::SerializedEvent;

use orchy_core::agent::{Agent, AgentId};
use orchy_core::knowledge::{Knowledge, KnowledgeId};
use orchy_core::message::{Message, MessageId};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::{Organization, OrganizationId};
use orchy_core::project::Project;
use orchy_core::resource_lock::ResourceLock;
use orchy_core::task::{ReviewId, ReviewRequest, Task, TaskId, TaskWatcher};

pub struct MemoryBackend {
    pub(crate) agents: RwLock<HashMap<AgentId, Agent>>,
    pub(crate) tasks: RwLock<HashMap<TaskId, Task>>,
    pub(crate) messages: RwLock<HashMap<MessageId, Message>>,
    pub(crate) message_receipts: RwLock<HashSet<(MessageId, AgentId)>>,
    pub(crate) projects: RwLock<HashMap<ProjectId, Project>>,
    pub(crate) watchers: RwLock<Vec<TaskWatcher>>,
    pub(crate) reviews: RwLock<HashMap<ReviewId, ReviewRequest>>,
    pub(crate) knowledge_entries: RwLock<HashMap<KnowledgeId, Knowledge>>,
    pub(crate) resource_locks: RwLock<HashMap<(String, String, String, String), ResourceLock>>,
    pub(crate) namespaces: RwLock<HashSet<(String, String, String)>>,
    pub(crate) organizations: RwLock<HashMap<OrganizationId, Organization>>,
    pub(crate) events: RwLock<Vec<SerializedEvent>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            agents: RwLock::new(HashMap::new()),
            tasks: RwLock::new(HashMap::new()),
            messages: RwLock::new(HashMap::new()),
            message_receipts: RwLock::new(HashSet::new()),
            projects: RwLock::new(HashMap::new()),
            watchers: RwLock::new(Vec::new()),
            reviews: RwLock::new(HashMap::new()),
            knowledge_entries: RwLock::new(HashMap::new()),
            resource_locks: RwLock::new(HashMap::new()),
            namespaces: RwLock::new(HashSet::new()),
            organizations: RwLock::new(HashMap::new()),
            events: RwLock::new(Vec::new()),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn apply_cursor_pagination<T, F>(
    items: Vec<T>,
    page: &orchy_core::pagination::PageParams,
    id_fn: F,
) -> orchy_core::pagination::Page<T>
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
