use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore, Alias};
use orchy_core::error::{Error, Resource};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_lock::LockStore;

use crate::parse_namespace;

pub struct UnlockResourceCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub name: String,
    pub holder_agent_id: String,
}

pub struct UnlockResource {
    agents: Arc<dyn AgentStore>,
    store: Arc<dyn LockStore>,
}

impl UnlockResource {
    pub fn new(agents: Arc<dyn AgentStore>, store: Arc<dyn LockStore>) -> Self {
        Self { agents, store }
    }

    pub async fn execute(&self, cmd: UnlockResourceCommand) -> ApplicationResult<()> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let holder = if let Ok(id) = AgentId::from_str(&cmd.holder_agent_id) {
            id
        } else {
            let alias = Alias::new(&cmd.holder_agent_id).map_err(|_| {
                Error::invalid_input(format!("invalid agent id: {}", cmd.holder_agent_id))
            })?;
            self.agents
                .find_by_alias(&org_id, &project, &alias)
                .await?
                .ok_or_else(|| Error::NotFound {
                    resource: Resource::Agent,
                    id: cmd.holder_agent_id.to_string(),
                })?
                .id()
                .clone()
        };

        self.agents
            .find_by_id(&holder)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Agent,
                id: holder.to_string(),
            })?;

        let mut lock = self
            .store
            .find(&org_id, &project, &namespace, &cmd.name)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Lock,
                id: cmd.name.to_string(),
            })?;

        if !lock.is_held_by(&holder) && !lock.is_expired() {
            return Err(
                Error::conflict(format!("lock '{}' is held by another agent", cmd.name)).into(),
            );
        }

        lock.mark_released()?;
        self.store.save(&mut lock).await?;
        self.store
            .delete(&org_id, &project, &namespace, &cmd.name)
            .await
            .map_err(Into::into)
    }
}
