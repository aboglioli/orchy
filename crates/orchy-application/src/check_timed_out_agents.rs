use std::sync::Arc;

use orchy_core::agent::AgentStore;
use orchy_core::error::Result;

use crate::dto::AgentResponse;

pub struct CheckTimedOutAgents {
    agents: Arc<dyn AgentStore>,
}

impl CheckTimedOutAgents {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, timeout_secs: u64) -> Result<Vec<AgentResponse>> {
        let agents = self.agents.find_timed_out(timeout_secs).await?;
        Ok(agents.iter().map(AgentResponse::from).collect())
    }
}
