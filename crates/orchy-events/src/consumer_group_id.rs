use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConsumerGroupId(String);

impl ConsumerGroupId {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() {
            return Err(Error::InvalidConsumerGroupId("must not be empty".into()));
        }
        if s.len() > 64 {
            return Err(Error::InvalidConsumerGroupId("max 64 chars".into()));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(Error::InvalidConsumerGroupId(format!("invalid: {s}")));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ConsumerGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for ConsumerGroupId {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<ConsumerGroupId> for String {
    fn from(g: ConsumerGroupId) -> Self {
        g.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid() {
        assert!(ConsumerGroupId::new("heartbeat-monitor").is_ok());
        assert!(ConsumerGroupId::new("group_1").is_ok());
        assert!(ConsumerGroupId::new("g").is_ok());
    }

    #[test]
    fn invalid_empty() {
        assert!(ConsumerGroupId::new("").is_err());
    }

    #[test]
    fn invalid_too_long() {
        let s = "a".repeat(65);
        assert!(ConsumerGroupId::new(s).is_err());
    }

    #[test]
    fn invalid_charset() {
        assert!(ConsumerGroupId::new("Heartbeat").is_err());
        assert!(ConsumerGroupId::new("group.1").is_err());
        assert!(ConsumerGroupId::new("group/1").is_err());
    }

    #[test]
    fn boundary_64_chars() {
        let s = "a".repeat(64);
        assert!(ConsumerGroupId::new(s).is_ok());
    }

    #[test]
    fn try_from_string_roundtrip() {
        let g = ConsumerGroupId::new("my-group").unwrap();
        let s: String = g.clone().into();
        let g2 = ConsumerGroupId::try_from(s).unwrap();
        assert_eq!(g, g2);
    }
}
