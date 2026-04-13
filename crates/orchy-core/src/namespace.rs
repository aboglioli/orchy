use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    pub fn root() -> Self {
        Self("/".to_string())
    }

    fn validate(s: &str) -> Result<(), String> {
        if s == "/" {
            return Ok(());
        }
        if !s.starts_with('/') {
            return Err("namespace must start with '/'".to_string());
        }
        for part in s[1..].split('/') {
            if part.is_empty() {
                return Err(
                    "namespace parts must not be empty (check for trailing or double slashes)"
                        .to_string(),
                );
            }
            for ch in part.chars() {
                if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                    return Err(format!(
                        "invalid character '{ch}' in namespace part '{part}'"
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn is_root(&self) -> bool {
        self.0 == "/"
    }

    pub fn with_scope(&self, scope: &str) -> Result<Namespace, String> {
        if self.is_root() {
            Namespace::try_from(format!("/{scope}"))
        } else {
            Namespace::try_from(format!("{}/{scope}", self.0))
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
}

impl TryFrom<String> for Namespace {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::validate(&s)?;
        Ok(Namespace(s))
    }
}

impl TryFrom<&str> for Namespace {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_string())
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(String);

impl ProjectId {
    fn validate(s: &str) -> Result<(), String> {
        if s.is_empty() {
            return Err("project must not be empty".to_string());
        }
        if s.contains('/') {
            return Err("project must be a single segment without slashes".to_string());
        }
        for ch in s.chars() {
            if !ch.is_ascii_alphanumeric() && ch != '-' && ch != '_' {
                return Err(format!("invalid character '{ch}' in project"));
            }
        }
        Ok(())
    }
}

impl TryFrom<String> for ProjectId {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::validate(&s)?;
        Ok(ProjectId(s))
    }
}

impl TryFrom<&str> for ProjectId {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_string())
    }
}

impl From<ProjectId> for String {
    fn from(p: ProjectId) -> Self {
        p.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
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
        let ns = Namespace::try_from("/backend").unwrap();
        assert_eq!(ns.as_ref(), "/backend");
        assert!(!ns.is_root());
        assert_eq!(ns.depth(), 1);
    }

    #[test]
    fn valid_nested_namespace() {
        let ns = Namespace::try_from("/backend/auth").unwrap();
        assert_eq!(ns.as_ref(), "/backend/auth");
        assert_eq!(ns.depth(), 2);
    }

    #[test]
    fn namespace_must_start_with_slash() {
        assert!(Namespace::try_from("backend").is_err());
        assert!(Namespace::try_from("").is_err());
    }

    #[test]
    fn no_double_slashes() {
        assert!(Namespace::try_from("//backend").is_err());
        assert!(Namespace::try_from("/backend//auth").is_err());
    }

    #[test]
    fn no_trailing_slash() {
        assert!(Namespace::try_from("/backend/").is_err());
    }

    #[test]
    fn starts_with_root_matches_all() {
        let root = Namespace::root();
        let child = Namespace::try_from("/backend").unwrap();
        let deep = Namespace::try_from("/backend/auth").unwrap();
        assert!(child.starts_with(&root));
        assert!(deep.starts_with(&root));
        assert!(root.starts_with(&root));
    }

    #[test]
    fn starts_with_hierarchy() {
        let parent = Namespace::try_from("/backend").unwrap();
        let child = Namespace::try_from("/backend/auth").unwrap();
        let sibling = Namespace::try_from("/frontend").unwrap();

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
        let ns = Namespace::try_from("/backend").unwrap();
        let child = ns.with_scope("auth").unwrap();
        assert_eq!(child.as_ref(), "/backend/auth");
    }

    #[test]
    fn parent_of_root_is_root() {
        assert_eq!(Namespace::root().parent(), Namespace::root());
    }

    #[test]
    fn parent_of_child_is_root() {
        let ns = Namespace::try_from("/backend").unwrap();
        assert_eq!(ns.parent(), Namespace::root());
    }

    #[test]
    fn parent_of_nested() {
        let ns = Namespace::try_from("/backend/auth").unwrap();
        assert_eq!(ns.parent().as_ref(), "/backend");
    }

    #[test]
    fn valid_project() {
        let p = ProjectId::try_from("my-project").unwrap();
        assert_eq!(p.as_ref(), "my-project");
    }

    #[test]
    fn project_slashes_fail() {
        assert!(ProjectId::try_from("my/project").is_err());
    }
}
