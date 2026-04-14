use serde::Serialize;

pub const NAMESPACE: &str = "link";

pub const TOPIC_CREATED: &str = "link.created";
pub const TOPIC_DELETED: &str = "link.deleted";

#[derive(Serialize)]
pub struct ProjectLinkCreatedPayload {
    pub link_id: String,
    pub source_project: String,
    pub target_project: String,
    pub resource_types: Vec<String>,
}

#[derive(Serialize)]
pub struct ProjectLinkDeletedPayload {
    pub link_id: String,
}
