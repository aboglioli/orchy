use std::sync::Arc;

use orchy_core::{ActorId, ActorStore, Clock, DomainError, SessionStore, SessionToken};

use crate::error::ApplicationResult;

pub struct TouchActor {
    actors: Arc<dyn ActorStore>,
    sessions: Arc<dyn SessionStore>,
    clock: Arc<dyn Clock>,
}

impl TouchActor {
    pub fn new(
        actors: Arc<dyn ActorStore>,
        sessions: Arc<dyn SessionStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            actors,
            sessions,
            clock,
        }
    }

    pub async fn execute(&self, actor: &str, session: Option<&str>) -> ApplicationResult<()> {
        let id: ActorId = actor.parse()?;
        let now = self.clock.now();
        match self.actors.touch(&id, now).await {
            Ok(()) | Err(DomainError::NotFound { .. }) => {}
            Err(e) => return Err(e.into()),
        }
        if let Some(token) = session.map(str::parse::<SessionToken>).transpose()? {
            self.sessions.touch(&token, now).await?;
        }
        Ok(())
    }
}
