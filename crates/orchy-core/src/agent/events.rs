use serde::Serialize;

pub const NAMESPACE: &str = "agent";

pub const TOPIC_REGISTERED: &str = "agent.registered";
pub const TOPIC_SPAWNED: &str = "agent.spawned";
pub const TOPIC_DISCONNECTED: &str = "agent.disconnected";
pub const TOPIC_ROLES_CHANGED: &str = "agent.roles_changed";
pub const TOPIC_RESUMED: &str = "agent.resumed";
pub const TOPIC_STATUS_CHANGED: &str = "agent.status_changed";
pub const TOPIC_MOVED: &str = "agent.moved";

#[derive(Serialize)]
pub struct AgentRegisteredPayload {
    pub org_id: String,
    pub agent_id: String,
    pub project: String,
    pub namespace: String,
    pub roles: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentSpawnedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub parent_id: String,
    pub project: String,
    pub namespace: String,
    pub roles: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentDisconnectedPayload {
    pub org_id: String,
    pub agent_id: String,
}

#[derive(Serialize)]
pub struct AgentRolesChangedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub roles: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentResumedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub namespace: String,
    pub roles: Vec<String>,
}

#[derive(Serialize)]
pub struct AgentMovedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub namespace: String,
}

#[derive(Serialize)]
pub struct AgentStatusChangedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub status: String,
}
