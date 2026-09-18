use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

const MAX_LEN: usize = 200;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Title(String);

impl Title {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(DomainError::validation("title must not be empty"));
        }
        if value.chars().count() > MAX_LEN {
            return Err(DomainError::validation(format!(
                "title must be at most {MAX_LEN} characters"
            )));
        }
        if value.contains('\n') {
            return Err(DomainError::validation("title must be a single line"));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Title {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Title {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Title {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Title> for String {
    fn from(title: Title) -> Self {
        title.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trims_surrounding_whitespace() {
        assert_eq!(Title::new("  hello  ").unwrap().as_str(), "hello");
    }

    #[test]
    fn rejects_empty_multiline_and_overlong_titles() {
        assert!(Title::new("").is_err());
        assert!(Title::new("   ").is_err());
        assert!(Title::new("a\nb").is_err());
        assert!(Title::new("x".repeat(MAX_LEN)).is_ok());
        assert!(Title::new("x".repeat(MAX_LEN + 1)).is_err());
    }

    #[test]
    fn counts_characters_not_bytes() {
        assert!(Title::new("é".repeat(MAX_LEN)).is_ok());
    }
}
