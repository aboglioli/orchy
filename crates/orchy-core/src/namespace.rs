use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Namespace(String);

impl Namespace {
    fn validate(s: &str) -> Result<(), String> {
        if s.is_empty() {
            return Err("namespace must not be empty".to_string());
        }
        for part in s.split('/') {
            if part.is_empty() {
                return Err("namespace parts must not be empty (check for leading, trailing, or double slashes)".to_string());
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

    pub fn project(&self) -> &str {
        self.0.split('/').next().unwrap()
    }

    pub fn scopes(&self) -> &str {
        self.0.split_once('/').map(|(_, rest)| rest).unwrap_or("")
    }

    pub fn with_scope(&self, scope: &str) -> Result<Namespace, String> {
        Namespace::try_from(format!("{}/{scope}", self.0))
    }

    pub fn to_project(&self) -> ProjectId {
        ProjectId::from(self)
    }

    pub fn is_project_root(&self) -> bool {
        !self.0.contains('/')
    }

    pub fn starts_with(&self, prefix: &Namespace) -> bool {
        if self.0 == prefix.0 {
            return true;
        }
        self.0.starts_with(&format!("{}/", prefix.0))
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

impl From<&Namespace> for ProjectId {
    fn from(ns: &Namespace) -> Self {
        ProjectId(ns.project().to_string())
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
    fn valid_simple() {
        let ns = Namespace::try_from("myapp").unwrap();
        assert_eq!(ns.as_ref(), "myapp");
    }

    #[test]
    fn valid_hierarchical() {
        let ns = Namespace::try_from("myapp/tasks/processing").unwrap();
        assert_eq!(ns.as_ref(), "myapp/tasks/processing");
    }

    #[test]
    fn valid_with_hyphens_and_underscores() {
        let ns = Namespace::try_from("my-app/task_queue/v2").unwrap();
        assert_eq!(ns.as_ref(), "my-app/task_queue/v2");
    }

    #[test]
    fn empty_fails() {
        assert!(Namespace::try_from("").is_err());
    }

    #[test]
    fn empty_part_fails() {
        assert!(
            Namespace::try_from("my//app").is_err(),
            "double slash should fail"
        );
        assert!(
            Namespace::try_from("/myapp").is_err(),
            "leading slash should fail"
        );
        assert!(
            Namespace::try_from("myapp/").is_err(),
            "trailing slash should fail"
        );
    }

    #[test]
    fn invalid_chars_fail() {
        assert!(Namespace::try_from("my@app").is_err(), "@ should fail");
        assert!(Namespace::try_from("my.app").is_err(), ". should fail");
        assert!(Namespace::try_from("my app").is_err(), "space should fail");
    }

    #[test]
    fn starts_with_works() {
        let ns = Namespace::try_from("myapp/tasks/processing").unwrap();
        let prefix = Namespace::try_from("myapp/tasks").unwrap();
        let other = Namespace::try_from("myapp/other").unwrap();
        let exact = Namespace::try_from("myapp/tasks/processing").unwrap();

        assert!(ns.starts_with(&prefix));
        assert!(!ns.starts_with(&other));
        assert!(ns.starts_with(&exact));
        assert!(!prefix.starts_with(&ns));
    }

    #[test]
    fn project_returns_first_segment() {
        let root = Namespace::try_from("orchy").unwrap();
        assert_eq!(root.project(), "orchy");

        let scoped = Namespace::try_from("orchy/backend/auth").unwrap();
        assert_eq!(scoped.project(), "orchy");
    }

    #[test]
    fn scopes_returns_rest_after_project() {
        let root = Namespace::try_from("orchy").unwrap();
        assert_eq!(root.scopes(), "");

        let one = Namespace::try_from("orchy/backend").unwrap();
        assert_eq!(one.scopes(), "backend");

        let deep = Namespace::try_from("orchy/backend/auth").unwrap();
        assert_eq!(deep.scopes(), "backend/auth");
    }

    #[test]
    fn with_scope_appends() {
        let root = Namespace::try_from("orchy").unwrap();
        let scoped = root.with_scope("backend").unwrap();
        assert_eq!(scoped.as_ref(), "orchy/backend");

        let deeper = scoped.with_scope("auth").unwrap();
        assert_eq!(deeper.as_ref(), "orchy/backend/auth");
    }

    #[test]
    fn with_scope_validates() {
        let root = Namespace::try_from("orchy").unwrap();
        assert!(root.with_scope("").is_err());
        assert!(root.with_scope("bad scope").is_err());
    }

    #[test]
    fn is_project_root_works() {
        let root = Namespace::try_from("orchy").unwrap();
        assert!(root.is_project_root());

        let scoped = Namespace::try_from("orchy/backend").unwrap();
        assert!(!scoped.is_project_root());
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

    #[test]
    fn project_from_namespace() {
        let ns = Namespace::try_from("orchy/backend/auth".to_string()).unwrap();
        let p = ProjectId::from(&ns);
        assert_eq!(p.as_ref(), "orchy");
    }
}
