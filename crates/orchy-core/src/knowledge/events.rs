use std::collections::HashMap;

use serde::Serialize;

pub const NAMESPACE: &str = "knowledge";

pub const TOPIC_CREATED: &str = "knowledge.created";
pub const TOPIC_UPDATED: &str = "knowledge.updated";
pub const TOPIC_TAGGED: &str = "knowledge.tagged";
pub const TOPIC_TAG_REMOVED: &str = "knowledge.tag_removed";
pub const TOPIC_MOVED: &str = "knowledge.moved";
pub const TOPIC_RENAMED: &str = "knowledge.renamed";
pub const TOPIC_DELETED: &str = "knowledge.deleted";
pub const TOPIC_METADATA_SET: &str = "knowledge.metadata_set";
pub const TOPIC_KIND_CHANGED: &str = "knowledge.kind_changed";

#[derive(Serialize)]
pub struct KnowledgeCreatedPayload {
    pub entry_id: String,
    pub project: String,
    pub namespace: String,
    pub path: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub agent_id: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct KnowledgeUpdatedPayload {
    pub entry_id: String,
    pub path: String,
    pub title: String,
    pub content: String,
    pub version: u64,
    pub agent_id: Option<String>,
}

#[derive(Serialize)]
pub struct KnowledgeTaggedPayload {
    pub entry_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct KnowledgeTagRemovedPayload {
    pub entry_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct KnowledgeMovedPayload {
    pub entry_id: String,
    pub from_namespace: String,
    pub to_namespace: String,
}

#[derive(Serialize)]
pub struct KnowledgeRenamedPayload {
    pub entry_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Serialize)]
pub struct KnowledgeDeletedPayload {
    pub entry_id: String,
    pub path: String,
}

#[derive(Serialize)]
pub struct KnowledgeMetadataSetPayload {
    pub entry_id: String,
    pub key: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct KnowledgeKindChangedPayload {
    pub entry_id: String,
    pub path: String,
    pub old_kind: String,
    pub new_kind: String,
    pub version: u64,
}
