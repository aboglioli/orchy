use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SkillName(String);

impl SkillName {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.len() < 2 || value.len() > 48 {
            return Err(DomainError::validation(format!(
                "skill name `{value}` must be 2-48 characters"
            )));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::validation(format!(
                "skill name `{value}` may only contain lowercase letters, digits and `-`"
            )));
        }
        if value.starts_with('-') || value.ends_with('-') {
            return Err(DomainError::validation(format!(
                "skill name `{value}` must not start or end with `-`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SkillName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for SkillName {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for SkillName {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SkillName> for String {
    fn from(name: SkillName) -> Self {
        name.0
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Summary(String);

impl Summary {
    pub const MAX: usize = 160;

    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(DomainError::validation(
                "a skill needs a one-line summary: it is what an agent scans before opening it",
            ));
        }
        if value.contains('\n') {
            return Err(DomainError::validation(
                "a skill summary is one line; put the detail in the body",
            ));
        }
        if value.chars().count() > Self::MAX {
            return Err(DomainError::validation(format!(
                "a skill summary must be at most {} characters",
                Self::MAX
            )));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Summary {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Summary {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Summary> for String {
    fn from(summary: Summary) -> Self {
        summary.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_lowercased_and_trimmed_rather_than_refused() {
        assert_eq!(
            SkillName::new("  Code-Review ").unwrap().as_str(),
            "code-review"
        );
    }

    #[test]
    fn a_name_that_could_not_be_typed_back_is_refused() {
        for bad in [
            "a",
            "",
            "-leading",
            "trailing-",
            "has space",
            "Ünicode",
            &"x".repeat(49),
        ] {
            assert!(SkillName::new(bad).is_err(), "`{bad}` should be refused");
        }
    }

    #[test]
    fn a_summary_is_one_line_and_never_empty() {
        assert!(Summary::new("never edit an applied migration").is_ok());
        assert!(Summary::new("   ").is_err());
        assert!(Summary::new("two\nlines").is_err());
        assert!(Summary::new("x".repeat(Summary::MAX + 1)).is_err());
    }
}
