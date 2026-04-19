use std::str::FromStr;
use std::sync::Arc;

use tower_cookies::Cookies;

use orchy_application::{AuthResponse, GetCurrentUserCommand};
use orchy_core::user::{Email, OrgMembership, UserId};

use crate::auth::get_auth_token;
use crate::container::Container;

#[derive(Debug, Clone)]
pub struct UserAuth {
    pub user_id: UserId,
    pub email: Email,
    pub is_platform_admin: bool,
    pub memberships: Vec<OrgMembership>,
}

impl UserAuth {
    pub fn is_platform_admin(&self) -> bool {
        self.is_platform_admin
    }

    pub fn get_org_role(
        &self,
        org_id: &orchy_core::organization::OrganizationId,
    ) -> Option<orchy_core::user::OrgRole> {
        self.memberships
            .iter()
            .find(|m| m.org_id() == org_id)
            .map(|m| m.role())
    }
}

impl From<AuthResponse> for UserAuth {
    fn from(response: AuthResponse) -> Self {
        let user_id = UserId::from_str(&response.user.id).unwrap_or_else(|_| UserId::new());
        let email = Email::new(&response.user.email)
            .unwrap_or_else(|_| Email::new("unknown@localhost").unwrap());
        let is_platform_admin = response.user.is_platform_admin;

        let memberships = response
            .memberships
            .into_iter()
            .filter_map(|m| {
                let id = orchy_core::user::MembershipId::from_str(&m.id).ok()?;
                let user_id = UserId::from_str(&m.user_id).ok()?;
                let org_id = orchy_core::organization::OrganizationId::new(&m.org_id).ok()?;
                let role = m.role.parse::<orchy_core::user::OrgRole>().ok()?;
                let created_at = m.joined_at.parse().ok()?;

                Some(OrgMembership::restore(
                    orchy_core::user::RestoreOrgMembership {
                        id,
                        user_id,
                        org_id,
                        role,
                        created_at,
                    },
                ))
            })
            .collect();

        Self {
            user_id,
            email,
            is_platform_admin,
            memberships,
        }
    }
}

pub async fn extract_user_auth(cookies: &Cookies, container: &Arc<Container>) -> Option<UserAuth> {
    let token = get_auth_token(cookies)?;

    let encoder = container.jwt_encoder.as_ref()?;

    let claims = encoder.decode(&token).ok()?;

    let response = container
        .app
        .get_current_user
        .execute(GetCurrentUserCommand {
            user_id: claims.sub,
        })
        .await
        .ok()?;

    Some(UserAuth::from(response))
}
