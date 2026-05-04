use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Result};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;

use crate::dto::ResourceLockDto;
use crate::parse_namespace;

pub struct LockResourceCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub name: String,
    pub holder_agent_id: String,
    pub ttl_secs: Option<u64>,
}

pub struct LockResource {
    agents: Arc<dyn AgentStore>,
    store: Arc<dyn LockStore>,
}

impl LockResource {
    pub fn new(agents: Arc<dyn AgentStore>, store: Arc<dyn LockStore>) -> Self {
        Self { agents, store }
    }

    pub async fn execute(&self, cmd: LockResourceCommand) -> Result<ResourceLockDto> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let holder = if let Ok(id) = AgentId::from_str(&cmd.holder_agent_id) {
            id
        } else {
            let alias = Alias::new(&cmd.holder_agent_id).map_err(|_| {
                Error::InvalidInput(format!("invalid agent id: {}", cmd.holder_agent_id))
            })?;
            self.agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound(format!("agent alias @{}", cmd.holder_agent_id)))?
                .id()
                .clone()
        };

        self.agents
            .find_by_id(&holder)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {holder}")))?;
        let ttl_secs = cmd.ttl_secs.unwrap_or(300);

        let lock = self
            .store
            .acquire_if_free(&org_id, &project, &namespace, &cmd.name, &holder, ttl_secs)
            .await?
            .ok_or_else(|| Error::Conflict(format!("resource '{}' is locked", cmd.name)))?;
        Ok(ResourceLockDto::from(&lock))
    }
}
