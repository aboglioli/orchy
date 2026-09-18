use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if !value.starts_with('/') {
            return Err(DomainError::validation(format!(
                "namespace `{value}` must be slash-rooted"
            )));
        }
        if value.len() > 1 && value.ends_with('/') {
            return Err(DomainError::validation(format!(
                "namespace `{value}` must not have a trailing slash"
            )));
        }
        for segment in value.split('/').skip(1).filter(|s| !s.is_empty()) {
            if !segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(DomainError::validation(format!(
                    "namespace segment `{segment}` may only contain letters, digits, `-` and `_`"
                )));
            }
        }
        if value.contains("//") {
            return Err(DomainError::validation(format!(
                "namespace `{value}` has an empty segment"
            )));
        }
        Ok(Self(value.to_owned()))
    }

    pub fn root() -> Self {
        Self("/".to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn parent(&self) -> Option<Self> {
        if self.is_root() {
            return None;
        }
        Some(match self.0.rsplit_once('/') {
            Some(("", _)) => Self::root(),
            Some((head, _)) => Self(head.to_owned()),
            None => Self::root(),
        })
    }

    pub fn ancestors(&self) -> Vec<Self> {
        let mut chain = Vec::new();
        let mut current = self.clone();
        while let Some(parent) = current.parent() {
            chain.push(parent.clone());
            current = parent;
        }
        chain
    }

    pub fn contains(&self, other: &Self) -> bool {
        if self.is_root() {
            return true;
        }
        other.0 == self.0 || other.0.starts_with(&format!("{}/", self.0))
    }

    pub fn child(&self, segment: &str) -> Result<Self> {
        if self.is_root() {
            Self::new(format!("/{segment}"))
        } else {
            Self::new(format!("{}/{segment}", self.0))
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Namespace {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Namespace {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Namespace> for String {
    fn from(ns: Namespace) -> Self {
        ns.0
    }
}

impl Default for Namespace {
    fn default() -> Self {
        Self::root()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns(s: &str) -> Namespace {
        Namespace::new(s).unwrap()
    }

    #[test]
    fn must_be_slash_rooted() {
        assert!(Namespace::new("backend").is_err());
        assert!(Namespace::new("/backend").is_ok());
        assert!(Namespace::new("/").is_ok());
    }

    #[test]
    fn rejects_trailing_slashes_and_empty_segments() {
        assert!(Namespace::new("/backend/").is_err());
        assert!(Namespace::new("/backend//auth").is_err());
    }

    #[test]
    fn rejects_segments_with_punctuation() {
        assert!(Namespace::new("/back end").is_err());
        assert!(Namespace::new("/back.end").is_err());
        assert!(Namespace::new("/back-end_2").is_ok());
    }

    #[test]
    fn parent_walks_up_to_the_root_and_stops() {
        assert_eq!(ns("/backend/auth").parent(), Some(ns("/backend")));
        assert_eq!(ns("/backend").parent(), Some(ns("/")));
        assert_eq!(ns("/").parent(), None);
    }

    #[test]
    fn ancestors_are_ordered_nearest_first() {
        assert_eq!(
            ns("/backend/auth/jwt").ancestors(),
            vec![ns("/backend/auth"), ns("/backend"), ns("/")]
        );
    }

    #[test]
    fn root_contains_everything_and_a_branch_contains_its_own_subtree() {
        assert!(ns("/").contains(&ns("/backend/auth")));
        assert!(ns("/backend").contains(&ns("/backend")));
        assert!(ns("/backend").contains(&ns("/backend/auth")));
        assert!(!ns("/backend").contains(&ns("/backendish")));
        assert!(!ns("/backend/auth").contains(&ns("/backend")));
    }

    #[test]
    fn child_appends_a_segment_without_doubling_the_root_slash() {
        assert_eq!(ns("/").child("backend").unwrap(), ns("/backend"));
        assert_eq!(ns("/backend").child("auth").unwrap(), ns("/backend/auth"));
    }
}
