use serde::Serialize;

pub const NAMESPACE: &str = "skill";

pub const TOPIC_CREATED: &str = "skill.created";
pub const TOPIC_UPDATED: &str = "skill.updated";
pub const TOPIC_MOVED: &str = "skill.moved";
pub const TOPIC_DELETED: &str = "skill.deleted";

#[derive(Serialize)]
pub struct SkillCreatedPayload {
    pub project: String,
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct SkillUpdatedPayload {
    pub project: String,
    pub namespace: String,
    pub name: String,
    pub description: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct SkillMovedPayload {
    pub project: String,
    pub from_namespace: String,
    pub to_namespace: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct SkillDeletedPayload {
    pub project: String,
    pub namespace: String,
    pub name: String,
}
