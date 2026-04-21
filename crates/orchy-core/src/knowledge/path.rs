use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
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
