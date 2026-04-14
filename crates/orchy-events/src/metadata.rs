use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata(HashMap<String, String>);

impl Metadata {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.0.insert(key.into(), value.into());
        self
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
}

impl From<HashMap<String, String>> for Metadata {
    fn from(map: HashMap<String, String>) -> Self {
        Self(map)
    }
}
