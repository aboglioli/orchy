use std::fmt;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OrganizationId(String);

impl OrganizationId {
    pub const PLATFORM: &'static str = "_platform";

    pub fn new(s: impl Into<String>) -> Result<Self> {
        let s = s.into();
        if s.is_empty() {
            return Err(Error::InvalidOrganization("must not be empty".into()));
        }
        if !s
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(Error::InvalidOrganization(format!("invalid: {s}")));
        }
        Ok(Self(s))
    }

    pub fn platform() -> Self {
        Self(Self::PLATFORM.to_owned())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OrganizationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl TryFrom<String> for OrganizationId {
    type Error = Error;
    fn try_from(s: String) -> Result<Self> {
        Self::new(s)
    }
}

impl From<OrganizationId> for String {
    fn from(o: OrganizationId) -> Self {
        o.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_sentinel_is_constructible() {
        let p = OrganizationId::platform();
        assert_eq!(p.as_str(), "_platform");
    }

    #[test]
    fn platform_sentinel_round_trips_through_validation() {
        let parsed = OrganizationId::new("_platform").unwrap();
        assert_eq!(parsed, OrganizationId::platform());
    }
}
