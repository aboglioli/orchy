use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::error::{DomainError, Result};

pub trait IdGenerator: Send + Sync {
    fn generate(&self) -> Ulid;
}

#[derive(Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id(Ulid);

impl Id {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        if value.is_empty() {
            return Err(DomainError::validation("id must not be empty"));
        }
        Ulid::from_string(value)
            .map(Self)
            .map_err(|_| DomainError::validation(format!("`{value}` is not a valid ULID")))
    }

    pub fn generate(ids: &dyn IdGenerator) -> Self {
        Self(ids.generate())
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        DateTime::from_timestamp_millis(self.0.timestamp_ms() as i64).unwrap_or_default()
    }

    pub fn as_ulid(&self) -> Ulid {
        self.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Id {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for Id {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedIds(Ulid);

    impl IdGenerator for FixedIds {
        fn generate(&self) -> Ulid {
            self.0
        }
    }

    #[test]
    fn new_rejects_anything_that_is_not_a_ulid() {
        assert!(Id::new("").is_err());
        assert!(Id::new("   ").is_err());
        assert!(Id::new("not-a-ulid").is_err());
        assert!(Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").is_ok());
    }

    #[test]
    fn ids_sort_in_creation_order() {
        let mut ids: Vec<Id> = ["01ARZ3NDEKTSV4RRFFQ69G5FAV", "01BX5ZZKBKACTAV9WEVGEMMVRZ"]
            .iter()
            .map(|s| Id::new(s).unwrap())
            .collect();
        let expected = ids.clone();
        ids.reverse();
        ids.sort();
        assert_eq!(ids, expected, "ULIDs must sort lexicographically by time");
    }

    #[test]
    fn generate_uses_the_injected_generator() {
        let ulid = Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        assert_eq!(Id::generate(&FixedIds(ulid)).as_ulid(), ulid);
    }

    #[test]
    fn round_trips_through_string() {
        let id = Id::new("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        assert_eq!(Id::new(id.to_string()).unwrap(), id);
    }
}
