use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Email(String);

impl Email {
    pub fn new(s: &str) -> Result<Self> {
        let s = s.trim();

        if s.is_empty() {
            return Err(Error::invalid_input("email cannot be empty"));
        }

        if s.contains(char::is_whitespace) {
            return Err(Error::invalid_input("email cannot contain whitespace"));
        }

        let at_idx = s
            .find('@')
            .ok_or_else(|| Error::invalid_input("email must contain @"))?;

        let local = &s[..at_idx];
        let domain = &s[at_idx + 1..];

        if local.is_empty() {
            return Err(Error::invalid_input("email local part cannot be empty"));
        }

        if domain.is_empty() {
            return Err(Error::invalid_input("email domain cannot be empty"));
        }

        if !domain.contains('.') {
            return Err(Error::invalid_input("email domain must contain a dot"));
        }

        Ok(Self(s.to_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlainPassword(String);

impl PlainPassword {
    pub fn new(s: &str) -> Result<Self> {
        if s.len() < 8 {
            return Err(Error::invalid_input(
                "password must be at least 8 characters",
            ));
        }

        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HashedPassword(String);

impl HashedPassword {
    pub fn new(s: &str) -> Result<Self> {
        if s.is_empty() {
            return Err(Error::invalid_input("password hash cannot be empty"));
        }

        Ok(Self(s.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_valid() {
        let email = Email::new("test@example.com").unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }

    #[test]
    fn email_normalizes_case() {
        let email = Email::new("Test@Example.COM").unwrap();
        assert_eq!(email.as_str(), "test@example.com");
    }

    #[test]
    fn email_rejects_empty() {
        assert!(Email::new("").is_err());
    }

    #[test]
    fn email_rejects_whitespace() {
        assert!(Email::new("test @example.com").is_err());
    }

    #[test]
    fn email_rejects_no_at() {
        assert!(Email::new("testexample.com").is_err());
    }

    #[test]
    fn email_rejects_no_domain() {
        assert!(Email::new("test@").is_err());
    }

    #[test]
    fn email_rejects_no_local() {
        assert!(Email::new("@example.com").is_err());
    }

    #[test]
    fn password_min_length() {
        assert!(PlainPassword::new("short").is_err());
        assert!(PlainPassword::new("longenough").is_ok());
    }
}
