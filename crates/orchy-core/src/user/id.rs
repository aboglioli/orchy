use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(Uuid);

impl UserId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for UserId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for UserId {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(s)
            .map_err(|e| Error::invalid_input(format!("invalid user id: {}", e)))?;
        Ok(Self::from_uuid(uuid))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MembershipId(Uuid);

impl MembershipId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl Default for MembershipId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for MembershipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for MembershipId {
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let uuid = Uuid::parse_str(s)
            .map_err(|e| Error::invalid_input(format!("invalid membership id: {}", e)))?;
        Ok(Self::from_uuid(uuid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_id_new() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1.as_str(), id2.as_str());
    }

    #[test]
    fn user_id_from_str() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let id = UserId::from_str(uuid_str).unwrap();
        assert_eq!(id.as_str(), uuid_str);
    }

    #[test]
    fn user_id_from_str_invalid() {
        assert!(UserId::from_str("invalid").is_err());
    }

    #[test]
    fn membership_id_new() {
        let id1 = MembershipId::new();
        let id2 = MembershipId::new();
        assert_ne!(id1.as_str(), id2.as_str());
    }
}
