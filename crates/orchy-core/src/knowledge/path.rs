use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct KnowledgePath(String);

impl KnowledgePath {
    pub fn new(path: &str) -> Result<Self> {
        validate_path(path)?;
        Ok(Self(path.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KnowledgePath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<KnowledgePath> for String {
    fn from(p: KnowledgePath) -> Self {
        p.0
    }
}

impl TryFrom<String> for KnowledgePath {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        validate_path(&value)?;
        Ok(Self(value.to_lowercase()))
    }
}

impl FromStr for KnowledgePath {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl PartialEq<&str> for KnowledgePath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<KnowledgePath> for str {
    fn eq(&self, other: &KnowledgePath) -> bool {
        self == other.as_str()
    }
}

impl AsRef<str> for KnowledgePath {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::borrow::Borrow<str> for KnowledgePath {
    fn borrow(&self) -> &str {
        &self.0
    }
}

fn validate_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(Error::InvalidInput("path must not be empty".into()));
    }
    if path.starts_with('/') || path.ends_with('/') {
        return Err(Error::InvalidInput(
            "path must not start or end with '/'".into(),
        ));
    }
    if path.contains("//") {
        return Err(Error::InvalidInput("path must not contain '//'".into()));
    }
    for segment in path.split('/') {
        if segment.is_empty() {
            return Err(Error::InvalidInput("path contains empty segment".into()));
        }
        if !segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(Error::InvalidInput(format!(
                "invalid character in path segment: {segment}"
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_path() {
        assert!(KnowledgePath::new("").is_err());
    }

    #[test]
    fn rejects_leading_slash() {
        assert!(KnowledgePath::new("/leading-slash").is_err());
    }

    #[test]
    fn rejects_trailing_slash() {
        assert!(KnowledgePath::new("trailing-slash/").is_err());
    }

    #[test]
    fn accepts_hierarchical_path() {
        let p = KnowledgePath::new("auth/jwt-strategy").unwrap();
        assert_eq!(p.as_str(), "auth/jwt-strategy");
    }

    #[test]
    fn accepts_single_segment() {
        assert!(KnowledgePath::new("notes").is_ok());
    }

    #[test]
    fn normalizes_to_lowercase() {
        let p = KnowledgePath::new("Auth/JWT").unwrap();
        assert_eq!(p.as_str(), "auth/jwt");
    }

    #[test]
    fn rejects_double_slash() {
        assert!(KnowledgePath::new("a//b").is_err());
    }

    #[test]
    fn rejects_invalid_characters() {
        assert!(KnowledgePath::new("spacey path").is_err());
        assert!(KnowledgePath::new("special!chars").is_err());
    }

    #[test]
    fn accepts_hyphens_and_underscores() {
        assert!(KnowledgePath::new("auth/jwt-strategy").is_ok());
        assert!(KnowledgePath::new("my_notes/deep_dive").is_ok());
    }

    #[test]
    fn error_message_has_single_invalid_input_prefix() {
        let err = KnowledgePath::new("/leading-slash").unwrap_err();
        let msg = err.to_string();
        let count = msg.matches("invalid input:").count();
        assert_eq!(
            count, 1,
            "expected exactly one 'invalid input:' prefix, got: {msg}"
        );
    }

    #[test]
    fn serde_round_trip() {
        let p = KnowledgePath::new("auth/jwt-strategy").unwrap();
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(json, "\"auth/jwt-strategy\"");
        let back: KnowledgePath = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);
    }

    #[test]
    fn serde_rejects_invalid() {
        let r: std::result::Result<KnowledgePath, _> = serde_json::from_str("\"/leading-slash\"");
        assert!(r.is_err(), "leading slash must be rejected by serde");
    }
}
