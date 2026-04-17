use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::message::{Message, MessageStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::{Page, PageParams};

use crate::parse_namespace;

pub struct CheckSentMessagesCommand {
    pub agent_id: String,
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct CheckSentMessages {
    messages: Arc<dyn MessageStore>,
}

impl CheckSentMessages {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(&self, cmd: CheckSentMessagesCommand) -> Result<Page<Message>> {
        let agent_id = AgentId::from_str(&cmd.agent_id).map_err(Error::InvalidInput)?;
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let page = PageParams::new(cmd.after, cmd.limit);

        self.messages
            .find_sent(&agent_id, &org_id, &project, &namespace, page)
            .await
    }
}
