use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Topic(String);

impl Topic {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() {
            return Err(Error::InvalidTopic("must not be empty".into()));
        }
        for part in s.split('.') {
            if part.is_empty() {
                return Err(Error::InvalidTopic("empty segment".into()));
            }
            if !part.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-') {
                return Err(Error::InvalidTopic(format!("invalid segment: {part}")));
            }
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Topic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for Topic {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> { Self::new(s) }
}

impl From<Topic> for String {
    fn from(t: Topic) -> Self { t.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_topics() {
        assert!(Topic::new("task.created").is_ok());
        assert!(Topic::new("agent.roles_changed").is_ok());
        assert!(Topic::new("memory.updated").is_ok());
    }

    #[test]
    fn invalid_topics() {
        assert!(Topic::new("").is_err());
        assert!(Topic::new("task..created").is_err());
        assert!(Topic::new("Task.Created").is_err());
    }
}
