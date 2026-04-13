pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::{Namespace, ProjectId};

pub trait MemoryStore: Send + Sync {
    fn save(&self, entry: &MemoryEntry) -> impl Future<Output = Result<()>> + Send;
    fn find_by_key(
        &self,
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
    fn delete(&self, namespace: &Namespace, key: &str) -> impl Future<Output = Result<()>> + Send;
}

pub trait ContextStore: Send + Sync {
    fn save(&self, snapshot: &ContextSnapshot) -> impl Future<Output = Result<()>> + Send;
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
        Self(Uuid::new_v4())
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
    namespace: Namespace,
    key: String,
    value: String,
    version: Version,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    written_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl MemoryEntry {
    pub fn new(
        namespace: Namespace,
        key: String,
        value: String,
        written_by: Option<AgentId>,
    ) -> Self {
        let now = Utc::now();
        Self {
            namespace,
            key,
            value,
            version: Version::initial(),
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            written_by,
            created_at: now,
            updated_at: now,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        namespace: Namespace,
        key: String,
        value: String,
        version: Version,
        embedding: Option<Vec<f32>>,
        embedding_model: Option<String>,
        embedding_dimensions: Option<u32>,
        written_by: Option<AgentId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            namespace,
            key,
            value,
            version,
            embedding,
            embedding_model,
            embedding_dimensions,
            written_by,
            created_at,
            updated_at,
        }
    }

    pub fn update(&mut self, value: String, written_by: Option<AgentId>) {
        self.value = value;
        self.version = self.version.next();
        if let Some(author) = written_by {
            self.written_by = Some(author);
        }
        self.updated_at = Utc::now();
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
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
pub struct WriteMemory {
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
    agent_id: AgentId,
    namespace: Namespace,
    summary: String,
    embedding: Option<Vec<f32>>,
    embedding_model: Option<String>,
    embedding_dimensions: Option<u32>,
    metadata: HashMap<String, String>,
    created_at: DateTime<Utc>,
}

impl ContextSnapshot {
    pub fn new(
        agent_id: AgentId,
        namespace: Namespace,
        summary: String,
        metadata: HashMap<String, String>,
    ) -> Self {
        Self {
            id: SnapshotId::new(),
            agent_id,
            namespace,
            summary,
            embedding: None,
            embedding_model: None,
            embedding_dimensions: None,
            metadata,
            created_at: Utc::now(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: SnapshotId,
        agent_id: AgentId,
        namespace: Namespace,
        summary: String,
        embedding: Option<Vec<f32>>,
        embedding_model: Option<String>,
        embedding_dimensions: Option<u32>,
        metadata: HashMap<String, String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            agent_id,
            namespace,
            summary,
            embedding,
            embedding_model,
            embedding_dimensions,
            metadata,
            created_at,
        }
    }

    pub fn set_embedding(&mut self, embedding: Vec<f32>, model: String, dimensions: u32) {
        self.embedding = Some(embedding);
        self.embedding_model = Some(model);
        self.embedding_dimensions = Some(dimensions);
    }

    pub fn id(&self) -> SnapshotId {
        self.id
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
