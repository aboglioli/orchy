use std::collections::HashMap;

use serde::Serialize;

pub const NAMESPACE: &str = "/agent";

pub const TOPIC_REGISTERED: &str = "agent.registered";
pub const TOPIC_ROLES_CHANGED: &str = "agent.roles_changed";
pub const TOPIC_RESUMED: &str = "agent.resumed";
pub const TOPIC_CONTEXT_SWITCHED: &str = "agent.context_switched";
pub const TOPIC_METADATA_CHANGED: &str = "agent.metadata_changed";
pub const TOPIC_ALIAS_CHANGED: &str = "agent.alias_changed";

#[derive(Serialize)]
pub struct AgentRegisteredPayload {
    pub org_id: String,
    pub agent_id: String,
    pub alias: String,
    pub project: String,
    pub namespace: String,
    pub roles: Vec<String>,
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
pub struct AgentContextSwitchedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub old_project: String,
    pub new_project: String,
    pub old_namespace: String,
    pub new_namespace: String,
}

#[derive(Serialize)]
pub struct AgentMetadataChangedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub metadata: HashMap<String, String>,
}

#[derive(Serialize)]
pub struct AgentAliasChangedPayload {
    pub org_id: String,
    pub agent_id: String,
    pub new_alias: String,
}
