use crate::error::{Error, Result};
use crate::value_objects::ids::AgentId;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

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
        // Try parsing as UUID → Agent
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
