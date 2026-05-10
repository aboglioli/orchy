use std::fmt;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::partition::PartitionKey;

const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EventKey(String);

impl EventKey {
    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.len() > 1024 {
            return Err(Error::InvalidEventKey(
                "event key must not exceed 1024 characters".into(),
            ));
        }
        Ok(Self(s))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for EventKey {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<EventKey> for String {
    fn from(k: EventKey) -> Self {
        k.0
    }
}

impl PartitionKey for EventKey {
    fn partition(&self, total_partitions: NonZeroU32) -> u32 {
        let hash = self
            .0
            .as_bytes()
            .iter()
            .fold(FNV_OFFSET_BASIS, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
            });
        (hash % u64::from(total_partitions.get())) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key() {
        assert!(EventKey::new("task-123").is_ok());
        assert!(EventKey::new("agent-abc").is_ok());
        assert!(EventKey::new("org/project/name").is_ok());
    }

    #[test]
    fn empty_key_is_allowed() {
        assert!(EventKey::new("").is_ok());
    }

    #[test]
    fn too_long_key_fails() {
        let s = "a".repeat(1025);
        assert!(EventKey::new(s).is_err());
    }

    #[test]
    fn partition_is_deterministic() {
        let partitions = NonZeroU32::new(4).unwrap();
        let k = EventKey::new("user-42").unwrap();
        assert_eq!(k.partition(partitions), k.partition(partitions));
    }

    #[test]
    fn partition_stays_in_range() {
        let partitions = NonZeroU32::new(4).unwrap();
        let keys = ["a", "b", "c", "d", "e"];
        let values: Vec<u32> = keys
            .iter()
            .map(|k| EventKey::new(*k).unwrap().partition(partitions))
            .collect();
        assert!(values.iter().all(|p| *p < partitions.get()));
    }

    #[test]
    fn same_key_same_partition() {
        let partitions = NonZeroU32::new(8).unwrap();
        let k1 = EventKey::new("user-1").unwrap();
        let k2 = EventKey::new("user-1").unwrap();
        assert_eq!(k1.partition(partitions), k2.partition(partitions));
    }

    #[test]
    fn fnv_known_vector_remains_stable() {
        let partitions = NonZeroU32::new(16).unwrap();
        let k = EventKey::new("user-42").unwrap();
        assert_eq!(k.partition(partitions), 11);
    }
}
