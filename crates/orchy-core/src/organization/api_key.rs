use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

const PREFIX: &str = "sk_";
const SECRET_LEN: usize = 67; // "sk_" + 64 hex chars

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RawApiKey(String);

impl RawApiKey {
    pub fn new(s: String) -> Result<Self> {
        if !s.starts_with(PREFIX) || s.len() != SECRET_LEN {
            return Err(Error::invalid_input(
                "API key must be 'sk_' followed by 64 hex characters",
            ));
        }

        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn prefix(&self) -> &str {
        &self.0[..8] // "sk_" + 5 hex chars
    }

    pub fn suffix(&self) -> &str {
        &self.0[self.0.len() - 4..]
    }
}

impl fmt::Display for RawApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKeyHash(String);

impl ApiKeyHash {
    pub fn new(s: String) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::invalid_input("API key hash cannot be empty"));
        }

        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApiKeyHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ApiKeyPrefix(String);

impl ApiKeyPrefix {
    pub fn new(s: String) -> Result<Self> {
        if s.len() != 8 {
            return Err(Error::invalid_input(
                "API key prefix must be exactly 8 characters",
            ));
        }

        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ApiKeyPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_key_valid() {
        let key = RawApiKey::new(
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        assert_eq!(
            key.as_str(),
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890"
        );
    }

    #[test]
    fn raw_key_rejects_short() {
        assert!(RawApiKey::new("sk_short".to_string()).is_err());
    }

    #[test]
    fn raw_key_rejects_bad_prefix() {
        assert!(
            RawApiKey::new(
                "pk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn raw_key_prefix() {
        let key = RawApiKey::new(
            "sk_abcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890".to_string(),
        )
        .unwrap();
        assert_eq!(key.prefix(), "sk_abcde");
    }

    #[test]
    fn hash_valid() {
        let hash = ApiKeyHash::new("abc123".to_string()).unwrap();
        assert_eq!(hash.as_str(), "abc123");
    }

    #[test]
    fn hash_rejects_empty() {
        assert!(ApiKeyHash::new("".to_string()).is_err());
    }

    #[test]
    fn prefix_valid() {
        let prefix = ApiKeyPrefix::new("sk_abcde".to_string()).unwrap();
        assert_eq!(prefix.as_str(), "sk_abcde");
    }

    #[test]
    fn prefix_rejects_wrong_length() {
        assert!(ApiKeyPrefix::new("sk_abc".to_string()).is_err());
    }
}
