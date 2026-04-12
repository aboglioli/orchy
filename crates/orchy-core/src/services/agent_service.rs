use std::sync::Arc;

use crate::entities::{Agent, RegisterAgent};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::{AgentId, AgentStatus};

pub struct AgentService<S: Store> {
    store: Arc<S>,
}

impl<S: Store> AgentService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn register(&self, registration: RegisterAgent) -> Result<Agent> {
        self.store.register(registration).await
    }

    pub async fn get(&self, id: &AgentId) -> Result<Agent> {
        self.store
            .get_agent(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }

    pub async fn list(&self) -> Result<Vec<Agent>> {
        self.store.list_agents().await
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        self.store.heartbeat(id).await
    }

    pub async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        self.store.update_agent_status(id, status).await
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        self.store.disconnect(id).await
    }

    pub async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        self.store.find_timed_out(timeout_secs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::RegisterAgent;
    use crate::store::mock::MockStore;
    use crate::value_objects::Namespace;
    use std::collections::HashMap;

    fn make_registration() -> RegisterAgent {
        RegisterAgent {
            namespace: Namespace::try_from("orchy".to_string()).unwrap(),
            roles: vec!["tester".to_string()],
            description: "test agent".to_string(),
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn register_creates_agent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert_eq!(agent.status, AgentStatus::Online);
    }

    #[tokio::test]
    async fn get_returns_agent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let registered = service.register(make_registration()).await.unwrap();
        let result = service.get(&registered.id).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn get_returns_not_found() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let result = service.get(&AgentId::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn heartbeat_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.heartbeat(&agent.id).await.is_ok());
    }

    #[tokio::test]
    async fn disconnect_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.disconnect(&agent.id).await.is_ok());
    }

    #[tokio::test]
    async fn find_timed_out_returns_empty() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let result = service.find_timed_out(60).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn list_returns_registered_agents() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        service.register(make_registration()).await.unwrap();
        let result = service.list().await.unwrap();
        assert_eq!(result.len(), 1);
    }
}
