use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::edge::{RelationDirection, RelationType};
use crate::resource_ref::ResourceRef;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNeighborhood {
    pub anchor: ResourceRef,
    pub relations: Vec<Relation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub edge_id: String,
    pub rel_type: RelationType,
    pub direction: RelationDirection,
    pub depth: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via: Option<ResourceRef>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    pub peer: PeerEntity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerEntity {
    Task(TaskSummary),
    Knowledge(KnowledgeSummary),
    Agent(AgentSummary),
    Message(MessageSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub status: String,
    pub priority: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeSummary {
    pub id: String,
    pub title: String,
    pub entry_kind: String,
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSummary {
    pub id: String,
    pub description: String,
    pub status: String,
    pub roles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageSummary {
    pub id: String,
    pub body: String,
    pub status: String,
    pub from: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkParam {
    pub to_kind: String,
    pub to_id: String,
    pub rel_type: String,
}
