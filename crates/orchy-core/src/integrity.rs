use std::fmt;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::Result;
use crate::id::Id;

#[async_trait]
pub trait Integrity: Send + Sync {
    /// Files the stores skip because they cannot be read as the entity they claim to be.
    async fn unreadable(&self) -> Result<Vec<Problem>>;

    /// Everything wrong with the vault, unreadable files included.
    async fn problems(&self) -> Result<Vec<Problem>>;

    /// Repairs one problem when it is mechanical; `false` when it needs a person.
    async fn repair(&self, problem: &Problem) -> Result<bool>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProblemKind {
    Unreadable,
    UnknownType,
    InvalidField,
    MissingField,
    DuplicateId,
    Misplaced,
    MisnamedFile,
    DanglingEdge,
    ParentCycle,
    StaleRollup,
    InvertedSupersedes,
    ExpiredLease,
    OrphanedGuard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub kind: ProblemKind,
    pub location: String,
    pub id: Option<Id>,
    pub detail: String,
    pub fixable: bool,
}

impl ProblemKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::UnknownType => "unknown_type",
            Self::InvalidField => "invalid_field",
            Self::MissingField => "missing_field",
            Self::DuplicateId => "duplicate_id",
            Self::Misplaced => "misplaced",
            Self::MisnamedFile => "misnamed_file",
            Self::DanglingEdge => "dangling_edge",
            Self::ParentCycle => "parent_cycle",
            Self::StaleRollup => "stale_rollup",
            Self::InvertedSupersedes => "inverted_supersedes",
            Self::ExpiredLease => "expired_lease",
            Self::OrphanedGuard => "orphaned_guard",
        }
    }

    /// Whether `orchy doctor --fix` can repair it without a person deciding anything.
    pub fn is_mechanical(&self) -> bool {
        matches!(
            self,
            Self::Misplaced
                | Self::MisnamedFile
                | Self::StaleRollup
                | Self::InvertedSupersedes
                | Self::ExpiredLease
                | Self::OrphanedGuard
        )
    }
}

impl Problem {
    pub fn new(
        kind: ProblemKind,
        location: impl Into<String>,
        id: Option<Id>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            location: location.into(),
            id,
            detail: detail.into(),
            fixable: kind.is_mechanical(),
        }
    }
}

impl fmt::Display for ProblemKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_problem_is_fixable_exactly_when_its_kind_is_mechanical() {
        let unreadable = Problem::new(ProblemKind::Unreadable, "docs/x.md", None, "bad yaml");
        let misplaced = Problem::new(
            ProblemKind::Misplaced,
            "notes/x.md",
            None,
            "belongs in docs/",
        );
        assert!(!unreadable.fixable, "a person has to read a broken file");
        assert!(misplaced.fixable, "moving a file is mechanical");
    }

    #[test]
    fn a_problem_kind_serialises_by_its_wire_name() {
        assert_eq!(
            serde_json::to_value(ProblemKind::InvertedSupersedes).unwrap(),
            serde_json::json!("inverted_supersedes")
        );
    }
}
