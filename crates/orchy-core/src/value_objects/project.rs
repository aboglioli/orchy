use serde::{Deserialize, Serialize};
use std::fmt;

use super::Namespace;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Project(String);

impl Project {
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

impl TryFrom<String> for Project {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        Self::validate(&s)?;
        Ok(Project(s))
    }
}

impl TryFrom<&str> for Project {
    type Error = String;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        Self::try_from(s.to_string())
    }
}

impl From<&Namespace> for Project {
    fn from(ns: &Namespace) -> Self {
        Project(ns.project().to_string())
    }
}

impl From<Project> for String {
    fn from(p: Project) -> Self {
        p.0
    }
}

impl fmt::Display for Project {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Project {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_project() {
        let p = Project::try_from("my-project").unwrap();
        assert_eq!(p.as_ref(), "my-project");
    }

    #[test]
    fn valid_with_underscores() {
        let p = Project::try_from("my_project").unwrap();
        assert_eq!(p.as_ref(), "my_project");
    }

    #[test]
    fn empty_fails() {
        assert!(Project::try_from("").is_err());
    }

    #[test]
    fn slashes_fail() {
        assert!(Project::try_from("my/project").is_err());
    }

    #[test]
    fn special_chars_fail() {
        assert!(Project::try_from("my@project").is_err());
        assert!(Project::try_from("my project").is_err());
    }

    #[test]
    fn from_namespace() {
        let ns = Namespace::try_from("orchy/backend/auth".to_string()).unwrap();
        let p = Project::from(&ns);
        assert_eq!(p.as_ref(), "orchy");
    }

    #[test]
    fn from_root_namespace() {
        let ns = Namespace::try_from("orchy".to_string()).unwrap();
        let p = Project::from(&ns);
        assert_eq!(p.as_ref(), "orchy");
    }
}
