use crate::entities::{CreateMessage, Message};
use crate::error::Result;
use crate::value_objects::{AgentId, MessageId, Namespace};

pub trait MessageStore: Send + Sync {
    async fn send(&self, message: CreateMessage) -> Result<Message>;
    async fn check(&self, agent: &AgentId, namespace: Option<&Namespace>) -> Result<Vec<Message>>;
    async fn mark_read(&self, ids: &[MessageId]) -> Result<()>;
}
