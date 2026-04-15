use serde::Serialize;

pub const NAMESPACE: &str = "message";

pub const TOPIC_SENT: &str = "message.sent";
pub const TOPIC_DELIVERED: &str = "message.delivered";
pub const TOPIC_READ: &str = "message.read";

#[derive(Serialize)]
pub struct MessageSentPayload {
    pub org_id: String,
    pub message_id: String,
    pub project: String,
    pub namespace: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
}

#[derive(Serialize)]
pub struct MessageDeliveredPayload {
    pub org_id: String,
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct MessageReadPayload {
    pub org_id: String,
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub status: String,
}
