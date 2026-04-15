use serde::Serialize;

pub const NAMESPACE: &str = "project";

pub const TOPIC_CREATED: &str = "project.created";
pub const TOPIC_DESCRIPTION_UPDATED: &str = "project.description_updated";
pub const TOPIC_NOTE_ADDED: &str = "project.note_added";
pub const TOPIC_METADATA_SET: &str = "project.metadata_set";

#[derive(Serialize)]
pub struct ProjectCreatedPayload {
    pub project: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct ProjectDescriptionUpdatedPayload {
    pub project: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct ProjectNoteAddedPayload {
    pub project: String,
    pub body: String,
}

#[derive(Serialize)]
pub struct ProjectMetadataSetPayload {
    pub project: String,
    pub key: String,
    pub value: String,
}
