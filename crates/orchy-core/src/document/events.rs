use serde::Serialize;

pub const NAMESPACE: &str = "document";

pub const TOPIC_CREATED: &str = "document.created";
pub const TOPIC_UPDATED: &str = "document.updated";
pub const TOPIC_TAGGED: &str = "document.tagged";
pub const TOPIC_TAG_REMOVED: &str = "document.tag_removed";
pub const TOPIC_MOVED: &str = "document.moved";
pub const TOPIC_RENAMED: &str = "document.renamed";
pub const TOPIC_DELETED: &str = "document.deleted";

#[derive(Serialize)]
pub struct DocumentCreatedPayload {
    pub document_id: String,
    pub project: String,
    pub namespace: String,
    pub path: String,
    pub title: String,
}

#[derive(Serialize)]
pub struct DocumentUpdatedPayload {
    pub document_id: String,
    pub title: String,
}

#[derive(Serialize)]
pub struct DocumentTaggedPayload {
    pub document_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct DocumentTagRemovedPayload {
    pub document_id: String,
    pub tag: String,
}

#[derive(Serialize)]
pub struct DocumentMovedPayload {
    pub document_id: String,
    pub from_namespace: String,
    pub to_namespace: String,
}

#[derive(Serialize)]
pub struct DocumentRenamedPayload {
    pub document_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Serialize)]
pub struct DocumentDeletedPayload {
    pub document_id: String,
}
