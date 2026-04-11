use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::value_objects::{AgentId, AgentStatus, Namespace};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: AgentId,
    pub namespace: Option<Namespace>,
    pub roles: Vec<String>,
    pub description: String,
    pub status: AgentStatus,
    pub last_heartbeat: DateTime<Utc>,
    pub connected_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub namespace: Option<Namespace>,
    pub roles: Vec<String>,
    pub description: String,
    pub metadata: HashMap<String, String>,
}
