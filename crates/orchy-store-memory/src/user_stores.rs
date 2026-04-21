use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_core::organization::OrganizationId;
use orchy_core::user::{
    Email, MembershipId, OrgMembership, OrgMembershipStore, User, UserId, UserStore,
};

use crate::MemoryBackend;

#[async_trait]
impl UserStore for MemoryBackend {
    async fn save(&self, user: &mut User) -> Result<()> {
        let mut users = self.users.write().await;
        let mut by_email = self.user_by_email.write().await;

        let id = *user.id();
        let email = user.email().as_str().to_string();

        users.insert(id, user.clone());
        by_email.insert(email, id);

        user.drain_events();
        Ok(())
    }

    async fn find_by_id(&self, id: &UserId) -> Result<Option<User>> {
        let users = self.users.read().await;
        Ok(users.get(id).cloned())
    }

    async fn find_by_email(&self, email: &Email) -> Result<Option<User>> {
        let users = self.users.read().await;
        let by_email = self.user_by_email.read().await;

        Ok(by_email
            .get(email.as_str())
            .and_then(|id| users.get(id).cloned()))
    }

    async fn list_all(&self) -> Result<Vec<User>> {
        let users = self.users.read().await;
        Ok(users.values().cloned().collect())
    }
}

#[async_trait]
impl OrgMembershipStore for MemoryBackend {
    async fn save(&self, membership: &OrgMembership) -> Result<()> {
        let mut memberships = self.memberships.write().await;
        let mut by_user = self.memberships_by_user.write().await;
        let mut by_org = self.memberships_by_org.write().await;

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
        let memberships = self.memberships.read().await;
        Ok(memberships.get(id).cloned())
    }

    async fn find_by_user(&self, user_id: &UserId) -> Result<Vec<OrgMembership>> {
        let memberships = self.memberships.read().await;
        let by_user = self.memberships_by_user.read().await;

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
        let memberships = self.memberships.read().await;
        let by_org = self.memberships_by_org.read().await;

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
        let memberships = self.memberships.read().await;
        let by_user = self.memberships_by_user.read().await;

        Ok(by_user.get(user_id).and_then(|ids| {
            ids.iter()
                .filter_map(|id| memberships.get(id))
                .find(|m| m.org_id() == org_id)
                .cloned()
        }))
    }

    async fn delete(&self, id: &MembershipId) -> Result<()> {
        let mut memberships = self.memberships.write().await;
        let mut by_user = self.memberships_by_user.write().await;
        let mut by_org = self.memberships_by_org.write().await;

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
