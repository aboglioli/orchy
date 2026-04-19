use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::message::{MessageId, MessageStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::MessageResponse;

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

    pub async fn execute(&self, cmd: ListConversationCommand) -> Result<Vec<MessageResponse>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let message_id = cmd.message_id.parse::<MessageId>()?;

        let limit = cmd.limit.map(|l| l as usize);
        let messages = self.messages.find_thread(&message_id, limit).await?;

        if let Some(root) = messages.first()
            && (root.org_id() != &org_id || root.project() != &project)
        {
            return Err(Error::NotFound("message not found in project".to_string()));
        }

        Ok(messages.iter().map(MessageResponse::from).collect())
    }
}
