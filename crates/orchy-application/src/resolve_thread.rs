use std::sync::Arc;

use orchy_core::{ActorId, Clock, Id, MessageStore};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveThreadCommand {
    pub message_id: String,
    pub actor: String,
}

pub struct ResolveThread {
    messages: Arc<dyn MessageStore>,
    clock: Arc<dyn Clock>,
}

impl ResolveThread {
    pub fn new(messages: Arc<dyn MessageStore>, clock: Arc<dyn Clock>) -> Self {
        Self { messages, clock }
    }

    pub async fn execute(&self, cmd: ResolveThreadCommand) -> ApplicationResult<MessageDto> {
        let actor: ActorId = cmd.actor.parse()?;
        let anchor = self.messages.require(&Id::new(&cmd.message_id)?).await?;
        let mut root = self.messages.require(anchor.thread()).await?;

        root.resolve(actor, &*self.clock)?;
        self.messages.save(&mut root).await?;
        Ok(MessageDto::from(&root))
    }
}
