use crate::entities::{ContextSnapshot, CreateSnapshot};
use crate::error::Result;
use crate::value_objects::{AgentId, Namespace};

pub trait ContextStore: Send + Sync {
    async fn save(&self, snapshot: CreateSnapshot) -> Result<ContextSnapshot>;
    async fn load(&self, agent: &AgentId) -> Result<Option<ContextSnapshot>>;
    async fn list(
        &self,
        agent: Option<&AgentId>,
        namespace: &Namespace,
    ) -> Result<Vec<ContextSnapshot>>;
    async fn search(
        &self,
        query: &str,
        embedding: Option<&[f32]>,
        namespace: &Namespace,
        agent_id: Option<&AgentId>,
        limit: usize,
    ) -> Result<Vec<ContextSnapshot>>;
}
