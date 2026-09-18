use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Kind(String);

impl Kind {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.is_empty() {
            return Err(DomainError::validation("type must not be empty"));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::validation(format!(
                "type `{value}` may only contain lowercase letters, digits and `-`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Kind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Kind {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Kind> for String {
    fn from(kind: Kind) -> Self {
        kind.0
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Status(String);

impl Status {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.is_empty() {
            return Err(DomainError::validation("status must not be empty"));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
        {
            return Err(DomainError::validation(format!(
                "status `{value}` may only contain lowercase letters, digits, `-` and `_`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Status {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Status {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Status> for String {
    fn from(status: Status) -> Self {
        status.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldOwner {
    Author,
    Event,
}

impl FieldOwner {
    pub fn is_projected(&self) -> bool {
        matches!(self, Self::Event)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KindDefinition {
    pub statuses: Vec<Status>,
    pub requires: Vec<String>,
    pub projected: Vec<String>,
}

pub trait TypeRegistry: Send + Sync {
    fn get(&self, kind: &Kind) -> Option<&KindDefinition>;
    fn kinds(&self) -> Vec<&Kind>;

    fn require(&self, kind: &Kind) -> Result<&KindDefinition> {
        self.get(kind)
            .ok_or_else(|| DomainError::UnknownType(kind.to_string()))
    }

    fn statuses_for(&self, kind: &Kind) -> Option<&[Status]> {
        self.get(kind).map(|d| d.statuses.as_slice())
    }

    fn field_owner(&self, kind: &Kind, field: &str) -> FieldOwner {
        match self.get(kind) {
            Some(def) if def.projected.iter().any(|f| f == field) => FieldOwner::Event,
            _ => FieldOwner::Author,
        }
    }

    fn validate_status(&self, kind: &Kind, status: &Status) -> Result<()> {
        let def = self.require(kind)?;
        if def.statuses.is_empty() || def.statuses.contains(status) {
            return Ok(());
        }
        Err(DomainError::validation(format!(
            "`{status}` is not a valid status for `{kind}` (expected one of: {})",
            def.statuses
                .iter()
                .map(Status::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        )))
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticTypeRegistry(BTreeMap<Kind, KindDefinition>);

impl StaticTypeRegistry {
    pub fn new(entries: BTreeMap<Kind, KindDefinition>) -> Self {
        Self(entries)
    }

    pub fn builtin() -> Self {
        let mut entries = BTreeMap::new();
        let statuses = |names: &[&str]| -> Vec<Status> {
            names
                .iter()
                .map(|n| Status::new(n).expect("builtin status"))
                .collect()
        };
        let projected =
            |names: &[&str]| -> Vec<String> { names.iter().map(|n| (*n).to_owned()).collect() };

        let mut add = |name: &str, def: KindDefinition| {
            entries.insert(Kind::new(name).expect("builtin kind"), def);
        };

        let canon = statuses(&["draft", "active", "superseded", "archived"]);
        let common_projected = projected(&["superseded_by", "derives", "produced_by", "subtasks"]);

        for name in [
            "note",
            "decision",
            "discovery",
            "pattern",
            "document",
            "config",
            "reference",
            "plan",
            "log",
            "skill",
            "overview",
            "summary",
            "report",
            "context",
        ] {
            add(
                name,
                KindDefinition {
                    statuses: canon.clone(),
                    requires: vec![],
                    projected: common_projected.clone(),
                },
            );
        }

        add(
            "candidate",
            KindDefinition {
                statuses: statuses(&["proposed", "promoted", "rejected"]),
                requires: vec![],
                projected: common_projected.clone(),
            },
        );

        Self(entries)
    }
}

impl TypeRegistry for StaticTypeRegistry {
    fn get(&self, kind: &Kind) -> Option<&KindDefinition> {
        self.0.get(kind)
    }

    fn kinds(&self) -> Vec<&Kind> {
        self.0.keys().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(s: &str) -> Kind {
        Kind::new(s).unwrap()
    }

    fn status(s: &str) -> Status {
        Status::new(s).unwrap()
    }

    fn registry() -> StaticTypeRegistry {
        StaticTypeRegistry::builtin()
    }

    #[test]
    fn kind_and_status_normalise_case() {
        assert_eq!(kind("Decision").as_str(), "decision");
        assert_eq!(status("Superseded").as_str(), "superseded");
    }

    #[test]
    fn kind_rejects_underscores_but_status_allows_them() {
        assert!(Kind::new("a_b").is_err());
        assert!(Status::new("in_review").is_ok());
    }

    #[test]
    fn an_unregistered_type_is_rejected_by_name() {
        let err = registry().require(&kind("invented")).unwrap_err();
        assert!(matches!(err, DomainError::UnknownType(_)), "{err:?}");
    }

    #[test]
    fn a_status_outside_the_types_enum_is_refused() {
        let registry = registry();
        assert!(
            registry
                .validate_status(&kind("decision"), &status("active"))
                .is_ok()
        );
        let err = registry
            .validate_status(&kind("decision"), &status("in-flight"))
            .unwrap_err();
        assert!(err.to_string().contains("not a valid status"), "{err}");
    }

    #[test]
    fn projected_fields_are_owned_by_events_and_everything_else_by_the_author() {
        let registry = registry();
        assert_eq!(
            registry.field_owner(&kind("decision"), "superseded_by"),
            FieldOwner::Event
        );
        assert!(
            registry
                .field_owner(&kind("decision"), "superseded_by")
                .is_projected()
        );
        assert_eq!(
            registry.field_owner(&kind("decision"), "title"),
            FieldOwner::Author
        );
    }

    #[test]
    fn an_unknown_type_leaves_every_field_to_the_author() {
        assert_eq!(
            registry().field_owner(&kind("invented"), "superseded_by"),
            FieldOwner::Author
        );
    }

    #[test]
    fn candidates_have_their_own_lifecycle() {
        let registry = registry();
        assert!(
            registry
                .validate_status(&kind("candidate"), &status("promoted"))
                .is_ok()
        );
        assert!(
            registry
                .validate_status(&kind("candidate"), &status("active"))
                .is_err()
        );
    }
}
