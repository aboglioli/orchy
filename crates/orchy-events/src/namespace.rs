use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        let normalized = if s.is_empty() || s == "/" {
            "/".to_owned()
        } else if s.starts_with('/') {
            s
        } else {
            format!("/{s}")
        };
        Self::validate(&normalized)?;
        Ok(Self(normalized))
    }

    pub fn root() -> Self {
        Self("/".to_owned())
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn with_scope(&self, scope: &str) -> Result<Namespace> {
        if self.is_root() {
            Namespace::new(format!("/{scope}"))
        } else {
            Namespace::new(format!("{}/{scope}", self.0))
        }
    }

    pub fn parent(&self) -> Namespace {
        if self.is_root() {
            return self.clone();
        }
        match self.0.rfind('/') {
            Some(0) => Namespace::root(),
            Some(pos) => Namespace(self.0[..pos].to_string()),
            None => Namespace::root(),
        }
    }

    pub fn starts_with(&self, prefix: &Namespace) -> bool {
        if prefix.is_root() {
            return true;
        }
        if self.0 == prefix.0 {
            return true;
        }
        self.0.starts_with(&format!("{}/", prefix.0))
    }

    pub fn depth(&self) -> usize {
        if self.is_root() {
            return 0;
        }
        self.0[1..].split('/').count()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(s: &str) -> Result<()> {
        if s == "/" {
            return Ok(());
        }
        if !s.starts_with('/') {
            return Err(Error::InvalidNamespace("must start with '/'".into()));
        }
        for part in s[1..].split('/') {
            if part.is_empty() {
                return Err(Error::InvalidNamespace(
                    "parts must not be empty (check for trailing or double slashes)".into(),
                ));
            }
            for ch in part.chars() {
                if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                    return Err(Error::InvalidNamespace(format!(
                        "invalid character '{ch}' in part '{part}'"
                    )));
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<String> for Namespace {
    type Error = Error;

    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<&str> for Namespace {
    type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl From<Namespace> for String {
    fn from(n: Namespace) -> Self {
        n.0
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Namespace {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl FromStr for Namespace {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_namespace() {
        let ns = Namespace::root();
        assert_eq!(ns.as_ref(), "/");
        assert!(ns.is_root());
        assert_eq!(ns.depth(), 0);
    }

    #[test]
    fn valid_namespace() {
        let ns = Namespace::new("/backend").unwrap();
        assert_eq!(ns.as_ref(), "/backend");
        assert!(!ns.is_root());
        assert_eq!(ns.depth(), 1);
    }

    #[test]
    fn valid_nested_namespace() {
        let ns = Namespace::new("/backend/auth").unwrap();
        assert_eq!(ns.as_ref(), "/backend/auth");
        assert_eq!(ns.depth(), 2);
    }

    #[test]
    fn namespace_auto_prepends_slash() {
        let ns = Namespace::new("backend").unwrap();
        assert_eq!(ns.as_ref(), "/backend");
    }

    #[test]
    fn empty_namespace_is_root() {
        let ns = Namespace::new("").unwrap();
        assert_eq!(ns.as_ref(), "/");
        assert!(ns.is_root());
    }

    #[test]
    fn no_double_slashes() {
        assert!(Namespace::new("//backend").is_err());
        assert!(Namespace::new("/backend//auth").is_err());
    }

    #[test]
    fn no_trailing_slash() {
        assert!(Namespace::new("/backend/").is_err());
    }

    #[test]
    fn starts_with_root_matches_all() {
        let root = Namespace::root();
        let child = Namespace::new("/backend").unwrap();
        let deep = Namespace::new("/backend/auth").unwrap();
        assert!(child.starts_with(&root));
        assert!(deep.starts_with(&root));
        assert!(root.starts_with(&root));
    }

    #[test]
    fn starts_with_hierarchy() {
        let parent = Namespace::new("/backend").unwrap();
        let child = Namespace::new("/backend/auth").unwrap();
        let sibling = Namespace::new("/frontend").unwrap();

        assert!(child.starts_with(&parent));
        assert!(!parent.starts_with(&child));
        assert!(!sibling.starts_with(&parent));
    }

    #[test]
    fn with_scope_from_root() {
        let root = Namespace::root();
        let child = root.with_scope("backend").unwrap();
        assert_eq!(child.as_ref(), "/backend");
    }

    #[test]
    fn with_scope_from_nested() {
        let ns = Namespace::new("/backend").unwrap();
        let child = ns.with_scope("auth").unwrap();
        assert_eq!(child.as_ref(), "/backend/auth");
    }

    #[test]
    fn parent_of_root_is_root() {
        assert_eq!(Namespace::root().parent(), Namespace::root());
    }

    #[test]
    fn parent_of_child_is_root() {
        let ns = Namespace::new("/backend").unwrap();
        assert_eq!(ns.parent(), Namespace::root());
    }

    #[test]
    fn parent_of_nested() {
        let ns = Namespace::new("/backend/auth").unwrap();
        assert_eq!(ns.parent().as_ref(), "/backend");
    }
}
