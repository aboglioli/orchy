use std::sync::Arc;

use super::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use crate::error::{Error, Result};

pub struct AgentService<S: AgentStore> {
    store: Arc<S>,
}

impl<S: AgentStore> AgentService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn register(&self, cmd: RegisterAgent) -> Result<Agent> {
        let agent = Agent::register(cmd.namespace, cmd.roles, cmd.description, cmd.metadata);
        self.store.save(&agent).await?;
        Ok(agent)
    }

    pub async fn get(&self, id: &AgentId) -> Result<Agent> {
        self.store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }

    pub async fn list(&self) -> Result<Vec<Agent>> {
        self.store.list().await
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.heartbeat();
        self.store.save(&agent).await
    }

    pub async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.update_status(status);
        self.store.save(&agent).await
    }

    pub async fn update_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        if roles.is_empty() {
            return Err(Error::InvalidInput("roles must not be empty".to_string()));
        }
        let mut agent = self.get(id).await?;
        agent.update_roles(roles);
        self.store.save(&agent).await?;
        Ok(agent)
    }

    pub async fn reconnect(
        &self,
        id: &AgentId,
        roles: Vec<String>,
        description: String,
    ) -> Result<Agent> {
        let mut agent = self.get(id).await?;
        agent.reconnect(roles, description);
        self.store.save(&agent).await?;
        Ok(agent)
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.disconnect();
        self.store.save(&agent).await
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
        assert_eq!(agent.status(), AgentStatus::Online);
    }

    #[tokio::test]
    async fn get_returns_not_found() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        assert!(service.get(&AgentId::new()).await.is_err());
    }

    #[tokio::test]
    async fn update_roles_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        let updated = service
            .update_roles(&agent.id(), vec!["reviewer".to_string()])
            .await
            .unwrap();
        assert_eq!(updated.roles(), &["reviewer"]);
    }

    #[tokio::test]
    async fn update_roles_fails_with_empty() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.update_roles(&agent.id(), vec![]).await.is_err());
    }
}
