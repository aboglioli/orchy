use std::sync::Arc;

use orchy_core::agent::{Agent, AgentStore};
use orchy_core::error::Result;

pub struct CheckTimedOutAgents {
    agents: Arc<dyn AgentStore>,
}

impl CheckTimedOutAgents {
    pub fn new(agents: Arc<dyn AgentStore>) -> Self {
        Self { agents }
    }

    pub async fn execute(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        self.agents.find_timed_out(timeout_secs).await
    }
}
