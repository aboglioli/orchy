use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{DomainError, Result};
use crate::id::Id;

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ActorAlias(String);

impl ActorAlias {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim().to_lowercase();
        if value.len() < 2 || value.len() > 32 {
            return Err(DomainError::validation(format!(
                "alias `{value}` must be 2-32 characters"
            )));
        }
        if !value
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(DomainError::validation(format!(
                "alias `{value}` may only contain lowercase letters, digits and `-`"
            )));
        }
        if value.starts_with('-') || value.ends_with('-') {
            return Err(DomainError::validation(format!(
                "alias `{value}` must not start or end with `-`"
            )));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActorAlias {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for ActorAlias {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for ActorAlias {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<ActorAlias> for String {
    fn from(alias: ActorAlias) -> Self {
        alias.0
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MachineId(Id);

impl MachineId {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        Id::new(value).map(Self)
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for MachineId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for MachineId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<MachineId> for String {
    fn from(id: MachineId) -> Self {
        id.to_string()
    }
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ActorId {
    alias: ActorAlias,
    machine: MachineId,
}

impl ActorId {
    pub fn new(alias: impl AsRef<str>, machine: impl AsRef<str>) -> Result<Self> {
        Ok(Self {
            alias: ActorAlias::new(alias)?,
            machine: MachineId::new(machine)?,
        })
    }

    pub fn alias(&self) -> &ActorAlias {
        &self.alias
    }

    pub fn machine(&self) -> &MachineId {
        &self.machine
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.alias, self.machine)
    }
}

impl FromStr for ActorId {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.strip_prefix('@').unwrap_or(s);
        let (alias, machine) = s.split_once('@').ok_or_else(|| {
            DomainError::validation(format!(
                "`{s}` is not an actor id (expected `alias@machine`)"
            ))
        })?;
        Self::new(alias, machine)
    }
}

impl TryFrom<String> for ActorId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl From<ActorId> for String {
    fn from(id: ActorId) -> Self {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    #[test]
    fn alias_is_normalised_to_lowercase() {
        assert_eq!(ActorAlias::new("Claude").unwrap().as_str(), "claude");
    }

    #[test]
    fn alias_rejects_bad_shapes() {
        assert!(ActorAlias::new("a").is_err(), "too short");
        assert!(ActorAlias::new("a".repeat(33)).is_err(), "too long");
        assert!(ActorAlias::new("cl aude").is_err(), "space");
        assert!(ActorAlias::new("cl_aude").is_err(), "underscore");
        assert!(ActorAlias::new("-claude").is_err(), "leading dash");
        assert!(ActorAlias::new("claude-").is_err(), "trailing dash");
        assert!(ActorAlias::new("claude-1").is_ok());
    }

    #[test]
    fn actor_id_renders_as_alias_at_machine() {
        let actor = ActorId::new("claude", MACHINE).unwrap();
        assert_eq!(actor.to_string(), format!("claude@{MACHINE}"));
    }

    #[test]
    fn actor_id_round_trips_and_tolerates_a_leading_at() {
        let actor = ActorId::new("claude", MACHINE).unwrap();
        assert_eq!(actor.to_string().parse::<ActorId>().unwrap(), actor);
        assert_eq!(
            format!("@claude@{MACHINE}").parse::<ActorId>().unwrap(),
            actor
        );
    }

    #[test]
    fn actor_id_needs_both_halves() {
        assert!("claude".parse::<ActorId>().is_err());
        assert!(format!("@{MACHINE}").parse::<ActorId>().is_err());
        assert!("claude@not-a-ulid".parse::<ActorId>().is_err());
    }

    #[test]
    fn the_same_alias_on_two_machines_is_two_actors() {
        let a = ActorId::new("claude", MACHINE).unwrap();
        let b = ActorId::new("claude", "01BX5ZZKBKACTAV9WEVGEMMVRZ").unwrap();
        assert_ne!(a, b, "a seat is per machine");
        assert_eq!(a.alias(), b.alias());
    }
}
