use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const CORRELATION_ID: &str = "correlation_id";
pub const CAUSATION_ID: &str = "causation_id";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata(HashMap<String, String>);

impl Metadata {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn try_with(mut self, key: impl Into<String>, value: impl Into<String>) -> Result<Self> {
        let key = key.into();
        Self::validate_key(&key)?;
        self.0.insert(key, value.into());
        Ok(self)
    }

    pub fn with(self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.try_with(key, value).expect("invalid metadata key")
    }

    fn validate_key(key: &str) -> Result<()> {
        if key.is_empty() || key.len() > 256 {
            return Err(Error::InvalidMetadataKey(format!(
                "metadata key must be 1-256 chars, got {}",
                key.len()
            )));
        }
        if !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '/')
        {
            return Err(Error::InvalidMetadataKey(format!(
                "metadata key must be ASCII alphanumeric, '/', '_' or '-': {key}"
            )));
        }
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn as_map(&self) -> &HashMap<String, String> {
        &self.0
    }

    pub fn into_map(self) -> HashMap<String, String> {
        self.0
    }

    pub fn correlation_id(&self) -> Option<&str> {
        self.get(CORRELATION_ID)
    }

    pub fn causation_id(&self) -> Option<&str> {
        self.get(CAUSATION_ID)
    }

    pub fn with_correlation_id(self, id: impl Into<String>) -> Result<Self> {
        self.try_with(CORRELATION_ID, id.into())
    }

    pub fn with_causation_id(self, id: impl Into<String>) -> Result<Self> {
        self.try_with(CAUSATION_ID, id.into())
    }
}

impl From<HashMap<String, String>> for Metadata {
    fn from(map: HashMap<String, String>) -> Self {
        Self(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_with_rejects_empty_key() {
        assert!(Metadata::new().try_with("", "value").is_err());
    }

    #[test]
    fn try_with_rejects_key_with_spaces() {
        assert!(Metadata::new().try_with("bad key", "value").is_err());
    }

    #[test]
    #[should_panic(expected = "invalid metadata key")]
    fn with_panics_on_invalid_key() {
        let _ = Metadata::new().with("bad key", "value");
    }

    #[test]
    fn try_with_accepts_namespaced_key() {
        let m = Metadata::new()
            .try_with("tenant/id", "abc")
            .unwrap()
            .try_with("agent_id", "x-1")
            .unwrap();
        assert_eq!(m.get("tenant/id"), Some("abc"));
        assert_eq!(m.get("agent_id"), Some("x-1"));
    }

    #[test]
    fn try_with_rejects_too_long_key() {
        let key = "a".repeat(257);
        assert!(Metadata::new().try_with(key, "value").is_err());
    }

    #[test]
    fn correlation_and_causation_ids_roundtrip_through_metadata() {
        let metadata = Metadata::new()
            .with_correlation_id("corr-1")
            .unwrap()
            .with_causation_id("cause-1")
            .unwrap();
        assert_eq!(metadata.correlation_id(), Some("corr-1"));
        assert_eq!(metadata.causation_id(), Some("cause-1"));
    }

    #[test]
    fn correlation_id_absent_when_not_set() {
        let metadata = Metadata::new();
        assert!(metadata.correlation_id().is_none());
        assert!(metadata.causation_id().is_none());
    }
}
