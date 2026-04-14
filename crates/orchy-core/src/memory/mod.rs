pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

use self::events as memory_events;

pub trait MemoryStore: Send + Sync {
    fn save(&self, entry: &mut MemoryEntry) -> impl Future<Output = Result<()>> + Send;
    fn find_by_key(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> impl Future<Output = Result<Option<MemoryEntry>>> + Send;
    fn list(&self, filter: MemoryFilter) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: Option<&Namespace>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<MemoryEntry>>> + Send;
    fn delete(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        key: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}

pub trait ContextStore: Send + Sync {
    fn save(&self, snapshot: &mut ContextSnapshot) -> impl Future<Output = Result<()>> + Send;
    fn find_latest(
        &self,
        agent: &AgentId,
    ) -> impl Future<Output = Result<Option<ContextSnapshot>>> + Send;
    fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<ContextSnapshot>>> + Send;
    fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<ContextSnapshot>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SnapshotId(Uuid);

impl SnapshotId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for SnapshotId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for SnapshotId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(u64);

impl Version {
    pub fn initial() -> Self {
        Version(1)
    }

    pub fn next(&self) -> Self {
        Version(self.0 + 1)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Version {
    fn from(v: u64) -> Self {
        Version(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    project: ProjectId,
    namespace: Namespace,
    key: String,
    value: String,
    version: Version,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    locked: bool,
    written_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl MemoryEntry {
    pub fn new(
        project: ProjectId,
        namespace: Namespace,
        key: String,
        value: String,
        written_by: Option<AgentId>,
    ) -> Result<Self> {
        if key.trim().is_empty() {
            return Err(Error::InvalidInput("memory key must not be empty".into()));
        }

        let now = Utc::now();
        let mut entry = Self {
            project,
            namespace,
            key,
            value,
            version: Version::initial(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            locked: false,
            written_by,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        entry.collector.collect(
            Event::create(
                entry.project.as_ref(),
                memory_events::NAMESPACE,
                memory_events::TOPIC_CREATED,
                Payload::from_json(&memory_events::MemoryCreatedPayload {
                    project: entry.project.to_string(),
                    namespace: entry.namespace.to_string(),
                    key: entry.key.clone(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(entry)
    }

    pub fn restore(r: RestoreMemoryEntry) -> Self {
        Self {
            project: r.project,
            namespace: r.namespace,
            key: r.key,
            value: r.value,
            version: r.version,
            embedding: r.embedding,
            embedding_model: r.embedding_model,
            embedding_dimensions: r.embedding_dimensions,
            locked: r.locked,
            written_by: r.written_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn update(&mut self, value: String, written_by: Option<AgentId>) -> Result<()> {
        if self.locked {
            return Err(Error::Conflict(format!(
                "memory entry '{}' is locked",
                self.key
            )));
        }
        self.value = value;
        self.version = self.version.next();
        if let Some(author) = written_by {
            self.written_by = Some(author);
        }
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_UPDATED,
            Payload::from_json(&memory_events::MemoryUpdatedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                key: self.key.clone(),
                version: self.version.as_u64(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));

        Ok(())
    }

    pub fn lock(&mut self) {
        self.locked = true;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_LOCKED,
            Payload::from_json(&memory_events::MemoryLockedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                key: self.key.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn unlock(&mut self) {
        self.locked = false;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_UNLOCKED,
            Payload::from_json(&memory_events::MemoryUnlockedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                key: self.key.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn mark_deleted(&mut self) {
        let _ = Event::create(
            self.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_DELETED,
            Payload::from_json(&memory_events::MemoryDeletedPayload {
                project: self.project.to_string(),
                namespace: self.namespace.to_string(),
                key: self.key.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn move_to(&mut self, namespace: Namespace) {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_MOVED,
            Payload::from_json(&memory_events::MemoryMovedPayload {
                project: self.project.to_string(),
                from_namespace,
                to_namespace: self.namespace.to_string(),
                key: self.key.clone(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn value(&self) -> &str {
        &self.value
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
    pub fn embedding_model(&self) -> Option<&str> {
        self.embedding_model.as_deref()
    }
    pub fn embedding_dimensions(&self) -> Option<u32> {
        self.embedding_dimensions
    }
    pub fn written_by(&self) -> Option<AgentId> {
        self.written_by
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone)]
pub struct RestoreMemoryEntry {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub version: Version,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub locked: bool,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct WriteMemory {
    pub project: ProjectId,
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub expected_version: Option<Version>,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<ProjectId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextSnapshot {
    id: SnapshotId,
    project: ProjectId,
    agent_id: AgentId,
    namespace: Namespace,
    summary: String,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl ContextSnapshot {
    pub fn new(
        project: ProjectId,
        agent_id: AgentId,
        namespace: Namespace,
        summary: String,
        metadata: HashMap<String, String>,
    ) -> Self {
        let mut snapshot = Self {
            id: SnapshotId::new(),
            project,
            agent_id,
            namespace,
            summary,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata,
            created_at: Utc::now(),
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            snapshot.project.as_ref(),
            memory_events::NAMESPACE,
            memory_events::TOPIC_CONTEXT_CAPTURED,
            Payload::from_json(&memory_events::ContextCapturedPayload {
                snapshot_id: snapshot.id.to_string(),
                agent_id: snapshot.agent_id.to_string(),
                project: snapshot.project.to_string(),
                namespace: snapshot.namespace.to_string(),
            })
            .unwrap(),
        )
        .map(|e| snapshot.collector.collect(e));

        snapshot
    }

    pub fn restore(r: RestoreContextSnapshot) -> Self {
        Self {
            id: r.id,
            project: r.project,
            agent_id: r.agent_id,
            namespace: r.namespace,
            summary: r.summary,
            embedding: r.embedding,
            embedding_model: r.embedding_model,
            embedding_dimensions: r.embedding_dimensions,
            metadata: r.metadata,
            created_at: r.created_at,
            collector: EventCollector::new(),
        }
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> SnapshotId {
        self.id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn embedding(&self) -> Option<&[f32]> {
        self.embedding.as_deref()
    }
    pub fn embedding_model(&self) -> Option<&str> {
        self.embedding_model.as_deref()
    }
    pub fn embedding_dimensions(&self) -> Option<u32> {
        self.embedding_dimensions
    }
    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

pub struct RestoreContextSnapshot {
    pub id: SnapshotId,
    pub project: ProjectId,
    pub agent_id: AgentId,
    pub namespace: Namespace,
    pub summary: String,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_project() -> ProjectId {
        ProjectId::try_from("test").unwrap()
    }

    fn test_namespace() -> Namespace {
        Namespace::root()
    }

    #[test]
    fn new_entry_has_initial_version() {
        let entry = MemoryEntry::new(
            test_project(),
            test_namespace(),
            "key".to_string(),
            "value".to_string(),
            None,
        )
        .unwrap();
        assert_eq!(entry.version().as_u64(), 1);
    }

    #[test]
    fn update_increments_version() {
        let mut entry = MemoryEntry::new(
            test_project(),
            test_namespace(),
            "key".to_string(),
            "value".to_string(),
            None,
        )
        .unwrap();
        entry.update("new value".to_string(), None).unwrap();
        assert_eq!(entry.version().as_u64(), 2);
    }

    #[test]
    fn update_changes_value() {
        let mut entry = MemoryEntry::new(
            test_project(),
            test_namespace(),
            "key".to_string(),
            "original".to_string(),
            None,
        )
        .unwrap();
        entry.update("updated".to_string(), None).unwrap();
        assert_eq!(entry.value(), "updated");
    }

    #[test]
    fn set_embedding_sets_fields() {
        let mut entry = MemoryEntry::new(
            test_project(),
            test_namespace(),
            "key".to_string(),
            "value".to_string(),
            None,
        )
        .unwrap();
        let embedding = vec![0.1, 0.2, 0.3];
        entry.set_embedding(embedding.clone(), "text-embedding-3".to_string(), 3);
        assert_eq!(entry.embedding(), Some(embedding.as_slice()));
        assert_eq!(entry.embedding_model(), Some("text-embedding-3"));
        assert_eq!(entry.embedding_dimensions(), Some(3));
    }
}
