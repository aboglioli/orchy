use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, MessageId, MessageTarget, Namespace};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageStatus {
    Pending,
    Delivered,
    Read,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub namespace: Option<Namespace>,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub status: MessageStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateMessage {
    pub namespace: Option<Namespace>,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
}
