use serde::Serialize;

pub const NAMESPACE: &str = "memory";

pub const TOPIC_CREATED: &str = "memory.created";
pub const TOPIC_UPDATED: &str = "memory.updated";
pub const TOPIC_LOCKED: &str = "memory.locked";
pub const TOPIC_UNLOCKED: &str = "memory.unlocked";
pub const TOPIC_MOVED: &str = "memory.moved";
pub const TOPIC_DELETED: &str = "memory.deleted";
pub const TOPIC_CONTEXT_CAPTURED: &str = "memory.context_captured";

#[derive(Serialize)]
pub struct MemoryCreatedPayload {
    pub project: String,
    pub namespace: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct MemoryUpdatedPayload {
    pub project: String,
    pub namespace: String,
    pub key: String,
    pub version: u64,
}

#[derive(Serialize)]
pub struct MemoryLockedPayload {
    pub project: String,
    pub namespace: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct MemoryUnlockedPayload {
    pub project: String,
    pub namespace: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct MemoryMovedPayload {
    pub project: String,
    pub from_namespace: String,
    pub to_namespace: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct MemoryDeletedPayload {
    pub project: String,
    pub namespace: String,
    pub key: String,
}

#[derive(Serialize)]
pub struct ContextCapturedPayload {
    pub snapshot_id: String,
    pub agent_id: String,
    pub project: String,
    pub namespace: String,
}
