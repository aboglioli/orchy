use std::sync::Arc;

use orchy_core::{ActorId, Clock, Id, MessageStore, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::MessageDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolveThreadCommand {
    pub message_id: String,
    pub actor: String,
}

pub struct ResolveThread {
    messages: Arc<dyn MessageStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl ResolveThread {
    pub fn new(
        messages: Arc<dyn MessageStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            messages,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: ResolveThreadCommand) -> ApplicationResult<MessageDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: ResolveThreadCommand) -> ApplicationResult<MessageDto> {
        let actor: ActorId = cmd.actor.parse()?;
        let anchor = self.messages.require(&Id::new(&cmd.message_id)?).await?;
        let mut root = self.messages.require(anchor.thread()).await?;

        root.resolve(actor, &*self.clock)?;
        self.messages.save(&mut root).await?;
        Ok(MessageDto::from(&root))
    }
}
