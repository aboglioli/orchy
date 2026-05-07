use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::message::{MessageId, MessageStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::MessageDto;

pub struct ListConversationCommand {
    pub org_id: String,
    pub project: String,
    pub message_id: String,
    pub limit: Option<u32>,
}

pub struct ListConversation {
    messages: Arc<dyn MessageStore>,
}

impl ListConversation {
    pub fn new(messages: Arc<dyn MessageStore>) -> Self {
        Self { messages }
    }

    pub async fn execute(
        &self,
        cmd: ListConversationCommand,
    ) -> ApplicationResult<Vec<MessageDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let message_id = cmd.message_id.parse::<MessageId>()?;

        let limit = cmd.limit.map(|l| l as usize);
        let messages = self.messages.find_thread(&message_id, limit).await?;

        if let Some(root) = messages.first()
            && (root.org_id() != &org_id || root.project() != &project)
        {
            return Err(Error::NotFound {
                resource: Resource::Project,
                id: String::new(),
            }
            .into());
        }

        Ok(messages.iter().map(MessageDto::from).collect())
    }
}
