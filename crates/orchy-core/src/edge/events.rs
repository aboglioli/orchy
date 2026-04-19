use serde::Serialize;

pub const NAMESPACE: &str = "/edge";

pub const TOPIC_CREATED: &str = "edge.created";
pub const TOPIC_INVALIDATED: &str = "edge.invalidated";

#[derive(Serialize)]
pub struct EdgeCreatedPayload {
    pub org_id: String,
    pub edge_id: String,
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub rel_type: String,
}

#[derive(Serialize)]
pub struct EdgeInvalidatedPayload {
    pub org_id: String,
    pub edge_id: String,
}
