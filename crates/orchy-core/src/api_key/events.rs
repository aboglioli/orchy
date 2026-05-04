use serde::Serialize;

pub const NAMESPACE: &str = "/api_key";

pub const TOPIC_CREATED: &str = "api_key.created";
pub const TOPIC_REVOKED: &str = "api_key.revoked";

#[derive(Serialize)]
pub struct ApiKeyCreatedPayload {
    pub org_id: String,
    pub api_key_id: String,
    pub name: String,
    pub user_id: Option<String>,
}

#[derive(Serialize)]
pub struct ApiKeyRevokedPayload {
    pub org_id: String,
    pub api_key_id: String,
}
