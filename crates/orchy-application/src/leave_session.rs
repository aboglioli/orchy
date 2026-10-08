use std::sync::Arc;

use orchy_core::{ActorId, Clock, SessionStore, SessionToken, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::SessionDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LeaveSessionCommand {
    pub actor: String,
    pub session: String,
}

pub struct LeaveSession {
    sessions: Arc<dyn SessionStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl LeaveSession {
    pub fn new(
        sessions: Arc<dyn SessionStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            sessions,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: LeaveSessionCommand) -> ApplicationResult<SessionDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: LeaveSessionCommand) -> ApplicationResult<SessionDto> {
        let actor: ActorId = cmd.actor.parse()?;
        let token: SessionToken = cmd.session.parse()?;
        let mut session = self.sessions.require_live(&token, self.clock.now()).await?;
        session.ensure_held_by(&actor)?;
        session.end(&*self.clock)?;
        self.sessions.save(&mut session).await?;
        Ok(SessionDto::from(&session))
    }
}
