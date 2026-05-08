use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::organization::{Organization, OrganizationId, OrganizationStore};
use orchy_core::user::{
    Email, OrgMembership, OrgMembershipStore, OrgRole, PasswordHasher, PlainPassword, User, UserId,
    UserStore,
};

use crate::dto::UserDto;

pub struct BootstrapAdmin {
    users: Arc<dyn UserStore>,
    orgs: Arc<dyn OrganizationStore>,
    memberships: Arc<dyn OrgMembershipStore>,
    hasher: Arc<dyn PasswordHasher>,
}

impl BootstrapAdmin {
    pub fn new(
        users: Arc<dyn UserStore>,
        orgs: Arc<dyn OrganizationStore>,
        memberships: Arc<dyn OrgMembershipStore>,
        hasher: Arc<dyn PasswordHasher>,
    ) -> Self {
        Self {
            users,
            orgs,
            memberships,
            hasher,
        }
    }

    pub async fn execute(&self) -> ApplicationResult<Option<UserDto>> {
        let existing_users = self.users.list_all().await?;
        if !existing_users.is_empty() {
            return Ok(None);
        }

        let email = Email::new("admin@orchy.sh")?;
        let password = PlainPassword::new("12345678")?;
        let id = UserId::new();

        let mut user = User::register_platform_admin(id, email, &password, self.hasher.as_ref())?;
        self.users.save(&mut user).await?;

        let org_id = OrganizationId::new("default")?;
        if self.orgs.find_by_id(&org_id).await?.is_none() {
            let mut org = Organization::new(org_id.clone(), "Default Organization".to_string())?;
            self.orgs.save(&mut org).await?;
        }

        if self.memberships.find(&id, &org_id).await?.is_none() {
            let mut membership = OrgMembership::new(id, org_id, OrgRole::Owner);
            self.memberships.save(&mut membership).await?;
        }

        Ok(Some(UserDto::from(&user)))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orchy_core::organization::OrganizationId;
    use orchy_core::user::{HashedPassword, PlainPassword};
    use orchy_store_memory::{
        MemoryOrgMembershipStore, MemoryOrganizationStore, MemoryState, MemoryUserStore,
    };

    use super::*;
    use orchy_core::error::DomainResult;

    struct NoopHasher;

    impl PasswordHasher for NoopHasher {
        fn hash(&self, plain: &PlainPassword) -> DomainResult<HashedPassword> {
            HashedPassword::new(plain.as_str())
        }

        fn verify(&self, plain: &PlainPassword, hashed: &HashedPassword) -> DomainResult<()> {
            if plain.as_str() == hashed.as_str() {
                return Ok(());
            }

            Err(orchy_core::error::DomainError::PasswordMismatch)
        }
    }

    #[tokio::test]
    async fn bootstrap_admin_creates_default_org_membership() {
        let state = Arc::new(MemoryState::new());
        let users = Arc::new(MemoryUserStore::new(Arc::clone(&state)));
        let orgs = Arc::new(MemoryOrganizationStore::new(Arc::clone(&state)));
        let memberships = Arc::new(MemoryOrgMembershipStore::new(state));
        let hasher = Arc::new(NoopHasher);

        let bootstrap = {
            #[allow(clippy::clone_on_ref_ptr)]
            BootstrapAdmin::new(users, orgs.clone(), memberships.clone(), hasher)
        };
        let admin = bootstrap.execute().await.unwrap().unwrap();

        let org_id = OrganizationId::new("default").unwrap();
        assert!(orgs.find_by_id(&org_id).await.unwrap().is_some());

        let membership = memberships
            .find(&admin.id.parse().unwrap(), &org_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(membership.role(), OrgRole::Owner);
    }
}
