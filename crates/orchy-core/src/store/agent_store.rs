use crate::entities::{Agent, RegisterAgent};
use crate::error::Result;
use crate::value_objects::{AgentId, AgentStatus};

pub trait AgentStore: Send + Sync {
    async fn register(&self, registration: RegisterAgent) -> Result<Agent>;
    async fn get(&self, id: &AgentId) -> Result<Option<Agent>>;
    async fn list(&self) -> Result<Vec<Agent>>;
    async fn heartbeat(&self, id: &AgentId) -> Result<()>;
    async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()>;
    async fn disconnect(&self, id: &AgentId) -> Result<()>;
    async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>>;
}
