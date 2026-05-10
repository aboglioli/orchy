use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ConsumerStream(String);

impl ConsumerStream {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() || s.len() > 128 {
            return Err(Error::Config("consumer stream must be 1-128 chars".into()));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '/' || c == '.')
        {
            return Err(Error::Config(format!("invalid consumer stream: {s}")));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for ConsumerStream {
    fn default() -> Self {
        Self("default".to_owned())
    }
}

impl fmt::Display for ConsumerStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for ConsumerStream {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<ConsumerStream> for String {
    fn from(value: ConsumerStream) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid() {
        assert!(ConsumerStream::new("default").is_ok());
        assert!(ConsumerStream::new("inbox").is_ok());
        assert!(ConsumerStream::new("heartbeat-monitor").is_ok());
        assert!(ConsumerStream::new("group_1").is_ok());
        assert!(ConsumerStream::new("backend/auth").is_ok());
        assert!(ConsumerStream::new("v1.events").is_ok());
        assert!(ConsumerStream::new("a").is_ok());
    }

    #[test]
    fn invalid_empty() {
        assert!(ConsumerStream::new("").is_err());
    }

    #[test]
    fn invalid_too_long() {
        let s = "a".repeat(129);
        assert!(ConsumerStream::new(s).is_err());
    }

    #[test]
    fn boundary_128_chars() {
        let s = "a".repeat(128);
        assert!(ConsumerStream::new(s).is_ok());
    }

    #[test]
    fn invalid_charset() {
        assert!(ConsumerStream::new("has space").is_err());
        assert!(ConsumerStream::new("has:colon").is_err());
        assert!(ConsumerStream::new("has@at").is_err());
    }

    #[test]
    fn default_is_literal_default() {
        let s = ConsumerStream::default();
        assert_eq!(s.as_str(), "default");
    }

    #[test]
    fn try_from_string_roundtrip() {
        let s = ConsumerStream::new("my/stream.v1").unwrap();
        let raw: String = s.clone().into();
        let s2 = ConsumerStream::try_from(raw).unwrap();
        assert_eq!(s, s2);
    }
}
