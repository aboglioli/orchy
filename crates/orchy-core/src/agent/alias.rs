use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Alias(String);

impl Alias {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.len() < 2 {
            return Err(Error::invalid_input("alias must be at least 2 characters"));
        }
        if s.len() > 64 {
            return Err(Error::invalid_input("alias must be at most 64 characters"));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(Error::invalid_input(
                "alias must be lowercase alphanumeric with hyphens only",
            ));
        }
        if s.starts_with('-') || s.ends_with('-') {
            return Err(Error::invalid_input(
                "alias must not start or end with hyphen",
            ));
        }
        Ok(Alias(s))
    }

    pub fn from_string_unchecked(s: impl Into<String>) -> Self {
        Alias(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Alias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Alias {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Alias::new(s)
    }
}

impl TryFrom<&str> for Alias {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        Alias::new(s)
    }
}

impl From<Alias> for String {
    fn from(a: Alias) -> Self {
        a.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_alias_shorter_than_two_chars() {
        assert!(Alias::new("a").is_err());
    }

    #[test]
    fn accepts_alias_at_minimum_length() {
        assert!(Alias::new("ab").is_ok());
    }

    #[test]
    fn accepts_alias_at_64_char_upper_bound() {
        assert!(Alias::new("a".repeat(64)).is_ok());
    }

    #[test]
    fn rejects_alias_longer_than_64_chars() {
        let err = Alias::new("a".repeat(65)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "invalid input: alias must be at most 64 characters"
        );
    }

    #[test]
    fn rejects_uppercase_chars() {
        assert!(Alias::new("Foo").is_err());
    }

    #[test]
    fn rejects_leading_or_trailing_hyphen() {
        assert!(Alias::new("-foo").is_err());
        assert!(Alias::new("foo-").is_err());
    }

    #[test]
    fn error_message_has_single_invalid_input_prefix() {
        let err = Alias::new("a").unwrap_err();
        let msg = err.to_string();
        let count = msg.matches("invalid input:").count();
        assert_eq!(
            count, 1,
            "expected exactly one 'invalid input:' prefix, got: {msg}"
        );
    }

    #[test]
    fn serde_round_trip() {
        let a = Alias::new("agent-7").unwrap();
        let json = serde_json::to_string(&a).unwrap();
        assert_eq!(json, "\"agent-7\"");
        let back: Alias = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }

    #[test]
    fn serde_rejects_invalid() {
        let r: std::result::Result<Alias, _> = serde_json::from_str("\"BadAlias\"");
        assert!(
            r.is_err(),
            "TryFrom<String> must reject uppercase via serde"
        );
    }
}
