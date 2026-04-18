use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::message::MessageStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{MessageResponse, PageResponse};
use crate::parse_namespace;

pub struct CheckMailboxCommand {
    pub agent_id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
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

    pub async fn execute(&self, cmd: CheckMailboxCommand) -> Result<PageResponse<MessageResponse>> {
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let page = PageParams::new(cmd.after, cmd.limit);

        let agent = self
            .agents
            .find_by_id(&agent_id)
            .await?
            .ok_or(Error::NotFound("agent not found".to_string()))?;
        let agent_roles = agent.roles().to_vec();

        let mut result = self
            .messages
            .find_pending(&agent_id, &agent_roles, &org_id, &project, &namespace, page)
            .await?;

        for msg in &mut result.items {
            if msg.is_directed_to(&agent_id) {
                msg.deliver()?;
                self.messages.save(msg).await?;
            }
        }

        Ok(PageResponse::from(result))
    }
}
