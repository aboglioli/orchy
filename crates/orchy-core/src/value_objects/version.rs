use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Version(u64);

impl Version {
    pub fn initial() -> Self {
        Version(1)
    }

    pub fn next(&self) -> Self {
        Version(self.0 + 1)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for Version {
    fn from(v: u64) -> Self {
        Version(v)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
