pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::Namespace;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MessageId(Uuid);

impl MessageId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for MessageId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MessageId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub enum MessageTarget {
    Agent(AgentId),
    Role(String),
    Broadcast,
}

impl MessageTarget {
    pub fn parse(s: &str) -> Result<Self> {
        if s == "broadcast" {
            return Ok(MessageTarget::Broadcast);
        }
        if let Some(role) = s.strip_prefix("role:") {
            if role.is_empty() {
                return Err(Error::InvalidInput(
                    "role name must not be empty".to_string(),
                ));
            }
            return Ok(MessageTarget::Role(role.to_string()));
        }
        match AgentId::from_str(s) {
            Ok(id) => Ok(MessageTarget::Agent(id)),
            Err(_) => Err(Error::InvalidInput(format!(
                "cannot parse message target: '{s}'"
            ))),
        }
    }
}

impl TryFrom<String> for MessageTarget {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Self::parse(&s)
    }
}

impl From<MessageTarget> for String {
    fn from(t: MessageTarget) -> Self {
        t.to_string()
    }
}

impl fmt::Display for MessageTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageTarget::Broadcast => write!(f, "broadcast"),
            MessageTarget::Role(r) => write!(f, "role:{r}"),
            MessageTarget::Agent(id) => write!(f, "{id}"),
        }
    }
}

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
    pub namespace: Namespace,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub reply_to: Option<MessageId>,
    pub status: MessageStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateMessage {
    pub namespace: Namespace,
    pub from: AgentId,
    pub to: MessageTarget,
    pub body: String,
    pub reply_to: Option<MessageId>,
}

pub trait MessageStore: Send + Sync {
    fn send(&self, message: CreateMessage) -> impl Future<Output = Result<Message>> + Send;
    fn check(
        &self,
        agent: &AgentId,
        namespace: &Namespace,
    ) -> impl Future<Output = Result<Vec<Message>>> + Send;
    fn mark_read(&self, ids: &[MessageId]) -> impl Future<Output = Result<()>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_broadcast() {
        let t = MessageTarget::parse("broadcast").unwrap();
        assert_eq!(t, MessageTarget::Broadcast);
    }

    #[test]
    fn parse_role() {
        let t = MessageTarget::parse("role:reviewer").unwrap();
        assert_eq!(t, MessageTarget::Role("reviewer".to_string()));
    }

    #[test]
    fn parse_agent_id() {
        let id = AgentId::new();
        let s = id.to_string();
        let t = MessageTarget::parse(&s).unwrap();
        assert_eq!(t, MessageTarget::Agent(id));
    }

    #[test]
    fn empty_role_fails() {
        let result = MessageTarget::parse("role:");
        assert!(result.is_err());
    }
}
