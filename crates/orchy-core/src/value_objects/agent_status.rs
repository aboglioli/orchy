use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentStatus {
    Online,
    Busy,
    Idle,
    Disconnected,
}

impl Default for AgentStatus {
    fn default() -> Self {
        AgentStatus::Online
    }
}

impl fmt::Display for AgentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AgentStatus::Online => "online",
            AgentStatus::Busy => "busy",
            AgentStatus::Idle => "idle",
            AgentStatus::Disconnected => "disconnected",
        };
        write!(f, "{s}")
    }
}
