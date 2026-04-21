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
