use std::fmt;

use serde::{Deserialize, Serialize};

use crate::organization::OrganizationId;

pub use orchy_events::Namespace;

#[async_trait::async_trait]
pub trait NamespaceStore: Send + Sync {
    async fn register(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
        namespace: &Namespace,
    ) -> crate::error::Result<()>;
    async fn list(
        &self,
        org: &OrganizationId,
        project: &ProjectId,
    ) -> crate::error::Result<Vec<Namespace>>;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ProjectId(String);

impl TryFrom<String> for ProjectId {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
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

impl std::str::FromStr for ProjectId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
