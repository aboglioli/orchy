use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventKey(String);

impl EventKey {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.len() > 1024 {
            return Err(Error::InvalidEventKey(
                "event key must not exceed 1024 characters".into(),
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for EventKey {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<EventKey> for String {
    fn from(k: EventKey) -> Self {
        k.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key() {
        assert!(EventKey::new("task-123").is_ok());
        assert!(EventKey::new("agent-abc").is_ok());
        assert!(EventKey::new("org/project/name").is_ok());
    }

    #[test]
    fn empty_key_is_allowed() {
        assert!(EventKey::new("").is_ok());
    }

    #[test]
    fn too_long_key_fails() {
        let s = "a".repeat(1025);
        assert!(EventKey::new(s).is_err());
    }
}
