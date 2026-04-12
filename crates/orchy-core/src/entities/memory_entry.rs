use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, Namespace, Project, Version};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub version: Version,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WriteMemory {
    pub namespace: Namespace,
    pub key: String,
    pub value: String,
    pub expected_version: Option<Version>,
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimensions: Option<u32>,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<Project>,
}
