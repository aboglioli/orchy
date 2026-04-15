use serde::Serialize;

pub const NAMESPACE: &str = "organization";
pub const TOPIC_CREATED: &str = "organization.created";
pub const TOPIC_API_KEY_ADDED: &str = "organization.api_key_added";
pub const TOPIC_API_KEY_REVOKED: &str = "organization.api_key_revoked";

#[derive(Serialize)]
pub struct OrgCreatedPayload {
    pub org_id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct ApiKeyAddedPayload {
    pub org_id: String,
    pub key_id: String,
    pub name: String,
}

#[derive(Serialize)]
pub struct ApiKeyRevokedPayload {
    pub org_id: String,
    pub key_id: String,
}
