#[allow(unused_imports)]
use std::str::FromStr;
use std::sync::Arc;

use super::{Agent, AgentId, AgentStatus, AgentStore, RegisterAgent};
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};
use crate::project::ProjectStore;

pub struct AgentService<S: AgentStore, PS: ProjectStore> {
    store: Arc<S>,
    project_store: Arc<PS>,
}

impl<S: AgentStore, PS: ProjectStore> AgentService<S, PS> {
    pub fn new(store: Arc<S>, project_store: Arc<PS>) -> Self {
        Self {
            store,
            project_store,
        }
    }

    pub async fn register(&self, cmd: RegisterAgent) -> Result<Agent> {
        if let Some(parent_id) = cmd.parent_id {
            let parent = self.get(&parent_id).await?;
            let mut agent =
                Agent::from_parent(&parent, cmd.namespace, cmd.roles, cmd.description, cmd.id)?;
            self.store.save(&mut agent).await?;
            return Ok(agent);
        }

        if let Some(ref id) = cmd.id
            && let Some(mut existing) = self.store.find_by_id(id).await?
            && *existing.org_id() == cmd.org_id
            && *existing.project() == cmd.project
        {
            existing.resume(cmd.namespace, cmd.roles, cmd.description)?;
            self.store.save(&mut existing).await?;
            return Ok(existing);
        }

        let mut agent = Agent::register(
            cmd.org_id,
            cmd.project,
            cmd.namespace,
            cmd.roles,
            cmd.description,
            cmd.id,
            cmd.metadata,
        )?;
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn get(&self, id: &AgentId) -> Result<Agent> {
        self.store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))
    }

    pub async fn list(&self, org: &OrganizationId, page: PageParams) -> Result<Page<Agent>> {
        self.store.list(org, page).await
    }

    pub async fn heartbeat(&self, id: &AgentId) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.heartbeat()?;
        self.store.save(&mut agent).await
    }

    pub async fn update_status(&self, id: &AgentId, status: AgentStatus) -> Result<()> {
        let mut agent = self.get(id).await?;
        agent.update_status(status)?;
        self.store.save(&mut agent).await
    }

    pub async fn change_roles(&self, id: &AgentId, roles: Vec<String>) -> Result<Agent> {
        if roles.is_empty() {
            return Err(Error::InvalidInput("roles must not be empty".to_string()));
        }
        let mut agent = self.get(id).await?;
        agent.change_roles(roles)?;
        self.store.save(&mut agent).await?;
        Ok(agent)
    }

    pub async fn switch_context(
        &self,
        id: &AgentId,
        org: &OrganizationId,
        project: Option<ProjectId>,
        namespace: Namespace,
    ) -> Result<Agent> {
        if let Some(ref p) = project {
            self.project_store
                .find_by_id(org, p)
                .await?
                .ok_or_else(|| Error::NotFound(format!("project {p}")))?;
        }

        let mut agent = self.get(id).await?;
        agent.switch_context(project, namespace)?;
        self.store.save(&mut agent).await?;
        Ok(agent)
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
        agent.disconnect()?;
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
        use orchy_events::OrganizationId;
        RegisterAgent {
            org_id: OrganizationId::new("orchy").unwrap(),
            project: ProjectId::try_from("orchy".to_string()).unwrap(),
            namespace: Namespace::root(),
            roles: vec!["tester".to_string()],
            description: "test agent".to_string(),
            id: None,
            parent_id: None,
            metadata: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn register_creates_agent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);
        let agent = service.register(make_registration()).await.unwrap();
        assert_eq!(agent.status(), AgentStatus::Online);
        assert!(agent.parent_id().is_none());
    }

    #[tokio::test]
    async fn register_from_parent() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);
        let parent = service.register(make_registration()).await.unwrap();

        let child_cmd = RegisterAgent {
            org_id: orchy_events::OrganizationId::new("orchy").unwrap(),
            project: ProjectId::try_from("orchy".to_string()).unwrap(),
            namespace: Namespace::try_from("/backend".to_string()).unwrap(),
            roles: vec!["reviewer".to_string()],
            description: "child agent".to_string(),
            id: None,
            parent_id: Some(parent.id().clone()),
            metadata: HashMap::new(),
        };
        let child = service.register(child_cmd).await.unwrap();
        assert_eq!(child.parent_id(), Some(parent.id()));
        assert_eq!(child.project().as_ref(), "orchy");
    }

    #[tokio::test]
    async fn register_resumes_by_id() {
        use orchy_events::OrganizationId;
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);

        let cmd = RegisterAgent {
            org_id: OrganizationId::new("orchy").unwrap(),
            project: ProjectId::try_from("orchy".to_string()).unwrap(),
            namespace: Namespace::root(),
            roles: vec!["coder".to_string()],
            description: "first session".to_string(),
            id: Some(AgentId::from_str("0192f3e4-4a3b-7c8d-9e0f-1a2b3c4d5e6f").unwrap()),
            parent_id: None,
            metadata: HashMap::new(),
        };
        let first = service.register(cmd.clone()).await.unwrap();

        let resume_cmd = RegisterAgent {
            description: "second session".to_string(),
            namespace: Namespace::try_from("/backend".to_string()).unwrap(),
            ..cmd
        };
        let resumed = service.register(resume_cmd).await.unwrap();

        assert_eq!(first.id(), resumed.id());
        assert_eq!(resumed.description(), "second session");
        assert_eq!(resumed.namespace().to_string(), "/backend");
        assert_eq!(resumed.status(), AgentStatus::Online);
    }

    #[tokio::test]
    async fn get_returns_not_found() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);
        assert!(service.get(&AgentId::new()).await.is_err());
    }

    #[tokio::test]
    async fn change_roles_succeeds() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);
        let agent = service.register(make_registration()).await.unwrap();
        let updated = service
            .change_roles(agent.id(), vec!["reviewer".to_string()])
            .await
            .unwrap();
        assert_eq!(updated.roles(), &["reviewer"]);
    }

    #[tokio::test]
    async fn change_roles_fails_with_empty() {
        let store = Arc::new(MockStore::default());
        let service = AgentService::new(store.clone(), store);
        let agent = service.register(make_registration()).await.unwrap();
        assert!(service.change_roles(agent.id(), vec![]).await.is_err());
    }
}
