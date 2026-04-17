use serde::Serialize;

pub const NAMESPACE: &str = "/lock";

pub const TOPIC_ACQUIRED: &str = "lock.acquired";
pub const TOPIC_RELEASED: &str = "lock.released";

#[derive(Serialize)]
pub struct LockAcquiredPayload {
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub name: String,
    pub holder: String,
    pub ttl_secs: u64,
}

#[derive(Serialize)]
pub struct LockReleasedPayload {
    pub org_id: String,
    pub project: String,
    pub namespace: String,
    pub name: String,
}
