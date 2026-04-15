use std::sync::Arc;

use super::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use crate::error::{Error, Result};
use crate::namespace::Namespace;

pub struct AgentService<S: AgentStore> {
    store: Arc<S>,
}

impl<S: AgentStore> AgentService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn register(&self, cmd: RegisterAgent) -> Result<Agent> {
        let mut agent = if let Some(parent_id) = cmd.parent_id {
            let parent = self.get(&parent_id).await?;
            Agent::from_parent(
                &parent,
                cmd.namespace,
                cmd.roles,
                cmd.description,
                cmd.alias,
            )
        } else {
            Agent::register(
                cmd.project,
                cmd.namespace,
                cmd.roles,
                cmd.description,
                cmd.alias,
                cmd.metadata,
            )
        };

        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn resume(
        &self,
        id: &AgentId,
        namespace: Namespace,
        roles: Vec<String>,
        description: String,
    ) -> Result<Agent> {
        let mut agent = self.get(id).await?;
        agent.resume(namespace, roles, description);
        self.store.save(&mut agent).await?;
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
        self.store.save(&mut agent).await
    }

    pub async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.update_status(status);
        self.store.save(&mut agent).await
    }

    pub async fn change_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        if roles.is_empty() {
            return Err(Error::InvalidInput("roles must not be empty".to_string()));
        }
        let mut agent = self.get(id).await?;
        agent.change_roles(roles);
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn move_to(&self, id: &AgentId, namespace: Namespace) -> Result<Agent> {
        let mut agent = self.get(id).await?;
        agent.move_to(namespace);
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn set_alias(&self, id: &AgentId, alias: Option<super::Alias>) -> Result<Agent> {
        let mut agent = self.get(id).await?;
        if let Some(ref a) = alias {
            if let Some(existing) = self.store.find_by_alias(agent.project(), a).await? {
                if existing.id() != *id {
                    return Err(crate::error::Error::Conflict(format!(
                        "alias '{}' already taken by agent {}",
                        a,
                        existing.id()
                    )));
                }
            }
        }
        agent.set_alias(alias);
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn find_by_alias(
        &self,
        project: &super::super::namespace::ProjectId,
        alias: &super::Alias,
    ) -> Result<Option<Agent>> {
        self.store.find_by_alias(project, alias).await
    }

    pub async fn update_metadata(
        &self,
        id: &AgentId,
        metadata: std::collections::HashMap<String, String>,
    ) -> Result<Agent> {
        let mut agent = self.get(id).await?;
        agent.set_metadata(metadata);
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn disconnect(&self, id: &AgentId) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.disconnect();
        self.store.save(&mut agent).await
    }

    pub async fn find_timed_out(&self, timeout_secs: u64) -> Result<Vec<Agent>> {
        self.store.find_timed_out(timeout_secs).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::MockStore;
    use crate::namespace::{Namespace, ProjectId};
    use std::collections::HashMap;

    fn make_registration() -> RegisterAgent {
        RegisterAgent {
            project: ProjectId::try_from("orchy".to_string()).unwrap(),
            namespace: Namespace::root(),
            roles: vec!["tester".to_string()],
            description: "test agent".to_string(),
            alias: None,
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn register_creates_agent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert_eq!(agent.status(), AgentStatus::Online);
        assert!(agent.parent_id().is_none());
    }

    #[tokio::test]
    async fn register_from_parent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let parent = service.register(make_registration()).await.unwrap();

        let child_cmd = RegisterAgent {
            project: ProjectId::try_from("orchy".to_string()).unwrap(),
            namespace: Namespace::try_from("/backend".to_string()).unwrap(),
            roles: vec!["reviewer".to_string()],
            description: "child agent".to_string(),
            alias: None,
            parent_id: Some(parent.id()),
            metadata: HashMap::new(),
        };
        let child = service.register(child_cmd).await.unwrap();
        assert_eq!(child.parent_id(), Some(parent.id()));
        assert_eq!(child.project().as_ref(), "orchy");
    }

    #[tokio::test]
    async fn get_returns_not_found() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        assert!(service.get(&AgentId::new()).await.is_err());
    }

    #[tokio::test]
    async fn change_roles_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        let updated = service
            .change_roles(&agent.id(), vec!["reviewer".to_string()])
            .await
            .unwrap();
        assert_eq!(updated.roles(), &["reviewer"]);
    }

    #[tokio::test]
    async fn change_roles_fails_with_empty() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.change_roles(&agent.id(), vec![]).await.is_err());
    }
}
