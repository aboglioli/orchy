use std::sync::Arc;

use super::{Agent, AgentId, AgentStatus, RegisterAgent};
use crate::error::{Error, Result};
use crate::store::Store;

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

    pub async fn update_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        if roles.is_empty() {
            return Err(Error::InvalidInput("roles must not be empty".to_string()));
        }
        self.store.update_agent_roles(id, roles).await
    }

    pub async fn reconnect(
        &self,
        id: &AgentId,
        roles: Vec<String>,
        description: String,
    ) -> Result<Agent> {
        self.store.reconnect(id, roles, description).await
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
    use crate::namespace::Namespace;
    use crate::store::mock::MockStore;
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

    #[tokio::test]
    async fn update_roles_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        let updated = service
            .update_roles(
                &agent.id,
                vec!["reviewer".to_string(), "analyzer".to_string()],
            )
            .await
            .unwrap();
        assert_eq!(updated.roles, vec!["reviewer", "analyzer"]);
    }

    #[tokio::test]
    async fn update_roles_fails_with_empty() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.update_roles(&agent.id, vec![]).await.is_err());
    }

    #[tokio::test]
    async fn update_roles_fails_for_unknown_agent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        assert!(
            service
                .update_roles(&AgentId::new(), vec!["tester".to_string()])
                .await
                .is_err()
        );
    }
}
