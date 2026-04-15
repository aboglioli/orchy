use serde::Serialize;

pub const NAMESPACE: &str = "message";

pub const TOPIC_SENT: &str = "message.sent";
pub const TOPIC_DELIVERED: &str = "message.delivered";
pub const TOPIC_READ: &str = "message.read";

#[derive(Serialize)]
pub struct MessageSentPayload {
    pub message_id: String,
    pub from: String,
    pub to: String,
    pub body: String,
    pub reply_to: Option<String>,
}

#[derive(Serialize)]
pub struct MessageDeliveredPayload {
    pub message_id: String,
}

#[derive(Serialize)]
pub struct MessageReadPayload {
    pub message_id: String,
}
