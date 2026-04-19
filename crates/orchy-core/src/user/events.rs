pub const NAMESPACE: &str = "/user";

pub const TOPIC_CREATED: &str = "user.created";
pub const TOPIC_LOGIN_SUCCEEDED: &str = "user.login_succeeded";
pub const TOPIC_LOGIN_FAILED: &str = "user.login_failed";
pub const TOPIC_PASSWORD_CHANGED: &str = "user.password_changed";
pub const TOPIC_DEACTIVATED: &str = "user.deactivated";
pub const TOPIC_MEMBERSHIP_ADDED: &str = "user.membership_added";
pub const TOPIC_PLATFORM_ADMIN_GRANTED: &str = "user.platform_admin_granted";
pub const TOPIC_MEMBERSHIP_ROLE_CHANGED: &str = "user.membership_role_changed";

use serde::Serialize;

#[derive(Serialize)]
pub struct UserCreatedPayload {
    pub user_id: String,
    pub email: String,
    pub is_platform_admin: bool,
}

#[derive(Serialize)]
pub struct UserLoginSucceededPayload {
    pub user_id: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct UserLoginFailedPayload {
    pub email: String,
    pub reason: String,
}

#[derive(Serialize)]
pub struct UserPasswordChangedPayload {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct UserDeactivatedPayload {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct UserMembershipAddedPayload {
    pub membership_id: String,
    pub user_id: String,
    pub org_id: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct UserPlatformAdminGrantedPayload {
    pub user_id: String,
}

#[derive(Serialize)]
pub struct UserMembershipRoleChangedPayload {
    pub membership_id: String,
    pub user_id: String,
    pub org_id: String,
    pub old_role: String,
    pub new_role: String,
}
