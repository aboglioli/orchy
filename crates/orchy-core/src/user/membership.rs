use crate::error::Result;
use crate::organization::OrganizationId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::id::{MembershipId, UserId};
use super::role::OrgRole;

#[async_trait::async_trait]
pub trait OrgMembershipStore: Send + Sync {
    async fn save(&self, membership: &OrgMembership) -> Result<()>;
    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>>;
    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>>;
    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>>;
    async fn find(
        &self,
        user_id: &UserId,
        org_id: &OrganizationId,
    ) -> Result<Option<OrgMembership>>;
    async fn delete(&self, id: &MembershipId) -> Result<()>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembership {
    id: MembershipId,
    user_id: UserId,
    org_id: OrganizationId,
    role: OrgRole,
    created_at: DateTime<Utc>,
}

pub struct RestoreOrgMembership {
    pub id: MembershipId,
    pub user_id: UserId,
    pub org_id: OrganizationId,
    pub role: OrgRole,
    pub created_at: DateTime<Utc>,
}

impl OrgMembership {
    pub fn new(user_id: UserId, org_id: OrganizationId, role: OrgRole) -> Self {
        Self {
            id: MembershipId::new(),
            user_id,
            org_id,
            role,
            created_at: Utc::now(),
        }
    }

    pub fn restore(r: RestoreOrgMembership) -> Self {
        Self {
            id: r.id,
            user_id: r.user_id,
            org_id: r.org_id,
            role: r.role,
            created_at: r.created_at,
        }
    }

    pub fn id(&self) -> &MembershipId {
        &self.id
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }

    pub fn role(&self) -> OrgRole {
        self.role
    }

    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub fn change_role(&mut self, new_role: OrgRole) {
        self.role = new_role;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_new() {
        let user_id = UserId::new();
        let org_id = OrganizationId::new("test-org").unwrap();
        let membership = OrgMembership::new(user_id, org_id.clone(), OrgRole::Member);

        assert_eq!(membership.role(), OrgRole::Member);
        assert_eq!(membership.user_id(), &user_id);
        assert_eq!(membership.org_id(), &org_id);
    }

    #[test]
    fn membership_change_role() {
        let user_id = UserId::new();
        let org_id = OrganizationId::new("test-org").unwrap();
        let mut membership = OrgMembership::new(user_id, org_id, OrgRole::Member);

        membership.change_role(OrgRole::Admin);
        assert_eq!(membership.role(), OrgRole::Admin);
    }
}
