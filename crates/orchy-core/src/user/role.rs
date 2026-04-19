use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
    Owner,
    Admin,
    Member,
}

impl OrgRole {
    pub fn can_invite(&self) -> bool {
        matches!(self, OrgRole::Owner | OrgRole::Admin)
    }

    pub fn can_remove_users(&self) -> bool {
        matches!(self, OrgRole::Owner | OrgRole::Admin)
    }

    pub fn can_manage_settings(&self) -> bool {
        matches!(self, OrgRole::Owner | OrgRole::Admin)
    }

    pub fn can_delete_org(&self) -> bool {
        matches!(self, OrgRole::Owner)
    }
}

impl fmt::Display for OrgRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrgRole::Owner => write!(f, "owner"),
            OrgRole::Admin => write!(f, "admin"),
            OrgRole::Member => write!(f, "member"),
        }
    }
}

impl std::str::FromStr for OrgRole {
    type Err = crate::error::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(OrgRole::Owner),
            "admin" => Ok(OrgRole::Admin),
            "member" => Ok(OrgRole::Member),
            _ => Err(crate::error::Error::invalid_input(format!(
                "invalid org role: {}",
                s
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_permissions() {
        assert!(OrgRole::Owner.can_invite());
        assert!(OrgRole::Owner.can_remove_users());
        assert!(OrgRole::Owner.can_manage_settings());
        assert!(OrgRole::Owner.can_delete_org());
    }

    #[test]
    fn admin_permissions() {
        assert!(OrgRole::Admin.can_invite());
        assert!(OrgRole::Admin.can_remove_users());
        assert!(OrgRole::Admin.can_manage_settings());
        assert!(!OrgRole::Admin.can_delete_org());
    }

    #[test]
    fn member_permissions() {
        assert!(!OrgRole::Member.can_invite());
        assert!(!OrgRole::Member.can_remove_users());
        assert!(!OrgRole::Member.can_manage_settings());
        assert!(!OrgRole::Member.can_delete_org());
    }

    #[test]
    fn parse_role() {
        assert_eq!("owner".parse::<OrgRole>().unwrap(), OrgRole::Owner);
        assert_eq!("admin".parse::<OrgRole>().unwrap(), OrgRole::Admin);
        assert_eq!("member".parse::<OrgRole>().unwrap(), OrgRole::Member);
        assert!("invalid".parse::<OrgRole>().is_err());
    }
}
