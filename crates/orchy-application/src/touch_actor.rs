use std::sync::Arc;

use orchy_core::{ActorId, ActorStore, Clock, DomainError};

use crate::error::ApplicationResult;

pub struct TouchActor {
    actors: Arc<dyn ActorStore>,
    clock: Arc<dyn Clock>,
}

impl TouchActor {
    pub fn new(actors: Arc<dyn ActorStore>, clock: Arc<dyn Clock>) -> Self {
        Self { actors, clock }
    }

    /// An actor that never announced has no presence to refresh.
    pub async fn execute(&self, actor: &str) -> ApplicationResult<()> {
        let id: ActorId = actor.parse()?;
        match self.actors.touch(&id, self.clock.now()).await {
            Ok(()) | Err(DomainError::NotFound { .. }) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
