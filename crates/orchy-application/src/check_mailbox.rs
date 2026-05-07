use std::str::FromStr;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Resource};
use orchy_core::message::MessageStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{MessageDto, PageResponse};

pub struct CheckMailboxCommand {
    pub agent_id: String,
    pub org_id: String,
    pub project: String,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct CheckMailbox {
    messages: Arc<dyn MessageStore>,
    agents: Arc<dyn AgentStore>,
}

impl CheckMailbox {
    pub fn new(messages: Arc<dyn MessageStore>, agents: Arc<dyn AgentStore>) -> Self {
        Self { messages, agents }
    }

    pub async fn execute(
        &self,
        cmd: CheckMailboxCommand,
    ) -> ApplicationResult<PageResponse<MessageDto>> {
        let agent_id = AgentId::from_str(&cmd.agent_id)?;
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let page = PageParams::new(cmd.after, cmd.limit);

        let agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or(Error::NotFound {
                resource: Resource::Agent,
                id: String::new(),
            })?;
        let agent_roles = agent.roles().to_vec();
        let agent_namespace = agent.namespace().clone();

        let result = self
            .messages
            .find_unread(
                &agent_id,
                &agent_roles,
                &agent_namespace,
                agent.user_id(),
                &org_id,
                &project,
                page,
            )
            .await?;

        Ok(PageResponse::from(result))
    }
}
