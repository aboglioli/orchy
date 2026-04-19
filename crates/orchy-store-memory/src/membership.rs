use std::collections::HashMap;
use std::sync::Mutex;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    MembershipId, OrgMembership, OrgMembershipStore, OrgRole, RestoreOrgMembership, UserId,
};

pub struct InMemoryMembershipStore {
    memberships: Mutex<HashMap<MembershipId, OrgMembership>>,
    by_user: Mutex<HashMap<UserId, Vec<MembershipId>>>,
    by_org: Mutex<HashMap<OrganizationId, Vec<MembershipId>>>,
}

impl InMemoryMembershipStore {
    pub fn new() -> Self {
        Self {
            memberships: Mutex::new(HashMap::new()),
            by_user: Mutex::new(HashMap::new()),
            by_org: Mutex::new(HashMap::new()),
        }
    }
}

impl Default for InMemoryMembershipStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl OrgMembershipStore for InMemoryMembershipStore {
    async fn save(&self, membership: &OrgMembership) -> Result<()> {
        let mut memberships = self.memberships.lock().unwrap();
        let mut by_user = self.by_user.lock().unwrap();
        let mut by_org = self.by_org.lock().unwrap();

        let id = *membership.id();
        let user_id = *membership.user_id();
        let org_id = membership.org_id().clone();

        memberships.insert(id, membership.clone());

        by_user.entry(user_id).or_default().retain(|m| *m != id);
        by_user.entry(user_id).or_default().push(id);

        let org_id2 = org_id.clone();
        by_org.entry(org_id).or_default().retain(|m| *m != id);
        by_org.entry(org_id2).or_default().push(id);

        Ok(())
    }

    async fn find_by_id(&self, id: &MembershipId) -> Result<Option<OrgMembership>> {
        let memberships = self.memberships.lock().unwrap();
        Ok(memberships.get(id).cloned())
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let memberships = self.memberships.lock().unwrap();
        let by_user = self.by_user.lock().unwrap();

        Ok(by_user
            .get(user_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| memberships.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn find_by_org(&self, org_id: &OrganizationId) -> Result<Vec<OrgMembership>> {
        let memberships = self.memberships.lock().unwrap();
        let by_org = self.by_org.lock().unwrap();

        Ok(by_org
            .get(org_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| memberships.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default())
    }

    async fn find(
        &self,
        user_id: &UserId,
        org_id: &OrganizationId,
    ) -> Result<Option<OrgMembership>> {
        let memberships = self.memberships.lock().unwrap();
        let by_user = self.by_user.lock().unwrap();

        Ok(by_user.get(user_id).and_then(|ids| {
            ids.iter()
                .filter_map(|id| memberships.get(id))
                .find(|m| m.org_id() == org_id)
                .cloned()
        }))
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        let mut memberships = self.memberships.lock().unwrap();
        let mut by_user = self.by_user.lock().unwrap();
        let mut by_org = self.by_org.lock().unwrap();

        if let Some(membership) = memberships.remove(id) {
            if let Some(ids) = by_user.get_mut(membership.user_id()) {
                ids.retain(|m| m != id);
            }
            if let Some(ids) = by_org.get_mut(membership.org_id()) {
                ids.retain(|m| m != id);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn membership_crud() {
        let store = InMemoryMembershipStore::new();
        let user_id = UserId::new();
        let org_id = OrganizationId::new("test-org").unwrap();

        let membership = OrgMembership::new(user_id, org_id.clone(), OrgRole::Member);
        store.save(&membership).await.unwrap();

        let found = store.find(&user_id, &org_id).await.unwrap();
        assert!(found.is_some());

        let by_user = store.find_by_user(&user_id).await.unwrap();
        assert_eq!(by_user.len(), 1);

        let by_org = store.find_by_org(&org_id).await.unwrap();
        assert_eq!(by_org.len(), 1);

        store.delete(membership.id()).await.unwrap();

        let found = store.find(&user_id, &org_id).await.unwrap();
        assert!(found.is_none());
    }
}
