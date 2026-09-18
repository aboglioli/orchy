use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::entity_ref::EntityKind;
use crate::error::{DomainError, Result};

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RelationType(String);

impl RelationType {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.is_empty() {
            return Err(DomainError::validation("relation must not be empty"));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        {
            return Err(DomainError::validation(format!(
                "relation `{value}` may only contain lowercase letters, digits and `_`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RelationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RelationType {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for RelationType {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<RelationType> for String {
    fn from(rel: RelationType) -> Self {
        rel.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arity {
    One,
    #[default]
    Many,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationDefinition {
    pub inverse: RelationType,
    pub from: Vec<EntityKind>,
    pub to: Vec<EntityKind>,
    pub symmetric: bool,
    pub arity: Arity,
}

impl RelationDefinition {
    pub fn accepts(&self, from: EntityKind, to: EntityKind) -> bool {
        (self.from.is_empty() || self.from.contains(&from))
            && (self.to.is_empty() || self.to.contains(&to))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_case_and_rejects_punctuation() {
        assert_eq!(
            RelationType::new("Depends_On").unwrap().as_str(),
            "depends_on"
        );
        assert!(RelationType::new("depends-on").is_err());
        assert!(RelationType::new("").is_err());
    }

    #[test]
    fn an_empty_endpoint_list_means_any_kind() {
        let def = RelationDefinition {
            inverse: RelationType::new("related_to").unwrap(),
            from: vec![],
            to: vec![],
            symmetric: true,
            arity: Arity::Many,
        };
        assert!(def.accepts(EntityKind::Document, EntityKind::Task));
        assert!(def.accepts(EntityKind::Actor, EntityKind::Message));
    }

    #[test]
    fn endpoints_are_enforced_when_declared() {
        let def = RelationDefinition {
            inverse: RelationType::new("subtasks").unwrap(),
            from: vec![EntityKind::Task],
            to: vec![EntityKind::Task],
            symmetric: false,
            arity: Arity::One,
        };
        assert!(def.accepts(EntityKind::Task, EntityKind::Task));
        assert!(!def.accepts(EntityKind::Document, EntityKind::Task));
        assert!(!def.accepts(EntityKind::Task, EntityKind::Document));
    }
}
