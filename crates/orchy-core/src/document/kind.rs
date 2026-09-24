use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Note,
    Decision,
    Discovery,
    Pattern,
    Document,
    Config,
    Reference,
    Plan,
    Log,
    Skill,
    Overview,
    Summary,
    Report,
    Context,
    Candidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocumentStatus {
    Draft,
    Active,
    Superseded,
    Archived,
    Proposed,
    Promoted,
    Rejected,
}

const CANON: [DocumentStatus; 4] = [
    DocumentStatus::Draft,
    DocumentStatus::Active,
    DocumentStatus::Superseded,
    DocumentStatus::Archived,
];

const CANDIDATE: [DocumentStatus; 3] = [
    DocumentStatus::Proposed,
    DocumentStatus::Promoted,
    DocumentStatus::Rejected,
];

/// Maintained from events: authoring one by hand is refused.
const PROJECTED: [&str; 4] = ["superseded_by", "derives", "produced_by", "subtasks"];

impl Kind {
    pub const ALL: [Self; 15] = [
        Self::Note,
        Self::Decision,
        Self::Discovery,
        Self::Pattern,
        Self::Document,
        Self::Config,
        Self::Reference,
        Self::Plan,
        Self::Log,
        Self::Skill,
        Self::Overview,
        Self::Summary,
        Self::Report,
        Self::Context,
        Self::Candidate,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Decision => "decision",
            Self::Discovery => "discovery",
            Self::Pattern => "pattern",
            Self::Document => "document",
            Self::Config => "config",
            Self::Reference => "reference",
            Self::Plan => "plan",
            Self::Log => "log",
            Self::Skill => "skill",
            Self::Overview => "overview",
            Self::Summary => "summary",
            Self::Report => "report",
            Self::Context => "context",
            Self::Candidate => "candidate",
        }
    }

    pub fn statuses(&self) -> &'static [DocumentStatus] {
        match self {
            Self::Candidate => &CANDIDATE,
            _ => &CANON,
        }
    }

    pub fn allows(&self, status: DocumentStatus) -> bool {
        self.statuses().contains(&status)
    }

    pub fn validate_status(&self, status: DocumentStatus) -> Result<()> {
        if self.allows(status) {
            return Ok(());
        }
        Err(DomainError::validation(format!(
            "`{status}` is not a status for `{self}` (expected one of: {})",
            self.statuses()
                .iter()
                .map(DocumentStatus::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }

    pub fn is_projected_field(field: &str) -> bool {
        PROJECTED.contains(&field)
    }

    pub fn is_candidate(&self) -> bool {
        matches!(self, Self::Candidate)
    }
}

impl DocumentStatus {
    pub const ALL: [Self; 7] = [
        Self::Draft,
        Self::Active,
        Self::Superseded,
        Self::Archived,
        Self::Proposed,
        Self::Promoted,
        Self::Rejected,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
            Self::Proposed => "proposed",
            Self::Promoted => "promoted",
            Self::Rejected => "rejected",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Kind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let name = s.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|k| k.as_str() == name)
            .ok_or(DomainError::UnknownType(name))
    }
}

impl fmt::Display for DocumentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for DocumentStatus {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let name = s.trim().to_lowercase();
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == name)
            .ok_or_else(|| DomainError::validation(format!("unknown document status: {name}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_and_status_round_trips() {
        for kind in Kind::ALL {
            assert_eq!(kind.as_str().parse::<Kind>().unwrap(), kind);
        }
        for status in DocumentStatus::ALL {
            assert_eq!(status.as_str().parse::<DocumentStatus>().unwrap(), status);
        }
    }

    #[test]
    fn an_invented_type_is_refused_by_name() {
        assert!(matches!(
            "invented".parse::<Kind>().unwrap_err(),
            DomainError::UnknownType(_)
        ));
    }

    #[test]
    fn a_candidate_has_its_own_lifecycle_and_shares_none_of_canon() {
        assert!(Kind::Candidate.allows(DocumentStatus::Promoted));
        assert!(!Kind::Candidate.allows(DocumentStatus::Active));
        assert!(Kind::Decision.allows(DocumentStatus::Active));
        assert!(!Kind::Decision.allows(DocumentStatus::Promoted));
    }

    #[test]
    fn every_kind_declares_at_least_one_status() {
        for kind in Kind::ALL {
            assert!(!kind.statuses().is_empty(), "{kind} has no statuses");
        }
    }

    #[test]
    fn candidate_statuses_and_canon_statuses_do_not_overlap() {
        for status in CANDIDATE {
            assert!(!CANON.contains(&status), "{status} is in both sets");
        }
    }

    #[test]
    fn validate_status_lists_the_alternatives_when_it_refuses() {
        let err = Kind::Decision
            .validate_status(DocumentStatus::Promoted)
            .unwrap_err();
        assert!(err.to_string().contains("draft"), "{err}");
        assert!(err.to_string().contains("archived"), "{err}");
    }

    #[test]
    fn projected_fields_are_recognised_and_ordinary_ones_are_not() {
        assert!(Kind::is_projected_field("superseded_by"));
        assert!(Kind::is_projected_field("subtasks"));
        assert!(!Kind::is_projected_field("title"));
        assert!(!Kind::is_projected_field("reviewer"));
    }

    #[test]
    fn only_candidate_is_a_candidate() {
        for kind in Kind::ALL {
            assert_eq!(kind.is_candidate(), kind == Kind::Candidate, "{kind}");
        }
    }
}
