use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::id::Id;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    Document,
    Task,
    Message,
    Skill,
    Actor,
}

impl EntityKind {
    pub fn is_content(&self) -> bool {
        matches!(
            self,
            Self::Document | Self::Task | Self::Message | Self::Skill
        )
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Document => "document",
            Self::Task => "task",
            Self::Message => "message",
            Self::Skill => "skill",
            Self::Actor => "actor",
        }
    }
}

impl fmt::Display for EntityKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for EntityKind {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "document" => Ok(Self::Document),
            "task" => Ok(Self::Task),
            "message" => Ok(Self::Message),
            "skill" => Ok(Self::Skill),
            "actor" => Ok(Self::Actor),
            other => Err(DomainError::validation(format!(
                "unknown entity kind: {other}"
            ))),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug, Serialize, Deserialize)]
pub struct EntityRef {
    kind: EntityKind,
    id: Id,
}

impl EntityRef {
    pub fn new(kind: EntityKind, id: Id) -> Self {
        Self { kind, id }
    }

    pub fn document(id: Id) -> Self {
        Self::new(EntityKind::Document, id)
    }

    pub fn task(id: Id) -> Self {
        Self::new(EntityKind::Task, id)
    }

    pub fn message(id: Id) -> Self {
        Self::new(EntityKind::Message, id)
    }

    pub fn kind(&self) -> EntityKind {
        self.kind
    }

    pub fn id(&self) -> &Id {
        &self.id
    }
}

impl fmt::Display for EntityRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

impl EntityRef {
    pub fn parse_or_assume(text: &str, assumed: Option<EntityKind>) -> Result<Self> {
        if let Some((kind, id)) = text.split_once(':') {
            return Ok(Self::new(kind.parse()?, Id::new(id)?));
        }
        let kind = assumed.ok_or_else(|| {
            DomainError::validation(format!(
                "`{text}` has no entity kind and the relation allows more than one, so it cannot be typed"
            ))
        })?;
        Ok(Self::new(kind, Id::new(text)?))
    }
}

impl FromStr for EntityRef {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let (kind, id) = s.split_once(':').ok_or_else(|| {
            DomainError::validation(format!("`{s}` is not an entity ref (expected `kind:id`)"))
        })?;
        Ok(Self::new(kind.parse()?, Id::new(id)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ULID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn round_trips_through_its_display_form() {
        let entity = EntityRef::task(Id::new(ULID).unwrap());
        assert_eq!(entity.to_string(), format!("task:{ULID}"));
        assert_eq!(entity.to_string().parse::<EntityRef>().unwrap(), entity);
    }

    #[test]
    fn rejects_a_ref_without_a_kind() {
        assert!(ULID.parse::<EntityRef>().is_err());
    }

    #[test]
    fn rejects_an_unknown_kind() {
        assert!(format!("widget:{ULID}").parse::<EntityRef>().is_err());
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    const ULID: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn an_explicit_prefix_wins_over_the_assumption() {
        let parsed =
            EntityRef::parse_or_assume(&format!("message:{ULID}"), Some(EntityKind::Task)).unwrap();
        assert_eq!(parsed.kind(), EntityKind::Message);
    }

    #[test]
    fn a_bare_id_takes_the_kind_the_relation_declares() {
        let parsed = EntityRef::parse_or_assume(ULID, Some(EntityKind::Task)).unwrap();
        assert_eq!(parsed.kind(), EntityKind::Task);
        assert_eq!(parsed.id().to_string(), ULID);
    }

    #[test]
    fn a_bare_id_with_nothing_to_assume_is_refused_rather_than_guessed() {
        let err = EntityRef::parse_or_assume(ULID, None).unwrap_err();
        assert!(err.to_string().contains("no entity kind"), "{err}");
    }

    #[test]
    fn an_unknown_prefix_is_an_error_not_a_fallback() {
        assert!(
            EntityRef::parse_or_assume(&format!("widget:{ULID}"), Some(EntityKind::Task)).is_err()
        );
    }

    #[test]
    fn a_malformed_id_is_refused_either_way() {
        assert!(EntityRef::parse_or_assume("task:nope", None).is_err());
        assert!(EntityRef::parse_or_assume("nope", Some(EntityKind::Task)).is_err());
    }
}
