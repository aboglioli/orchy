use serde::Serialize;

pub const NAMESPACE: &str = "/organization";
pub const TOPIC_CREATED: &str = "organization.created";

#[derive(Serialize)]
pub struct OrgCreatedPayload {
    pub org_id: String,
    pub name: String,
}
