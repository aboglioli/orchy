use std::sync::Arc;

use orchy_core::{ActorStore, Clock, SessionStore};
use serde::{Deserialize, Serialize};

use crate::dto::{ActorDto, SessionDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListActorsCommand {
    pub live_only: bool,
}

pub struct ListActors {
    actors: Arc<dyn ActorStore>,
    sessions: Arc<dyn SessionStore>,
    clock: Arc<dyn Clock>,
}

impl ListActors {
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

    pub async fn execute(&self, cmd: ListActorsCommand) -> ApplicationResult<Vec<ActorDto>> {
        let now = self.clock.now();
        let roster = self.actors.roster().await?;
        let present = self.actors.present(now).await?;
        let live: Vec<SessionDto> = self
            .sessions
            .all()
            .await?
            .iter()
            .filter(|s| s.is_live(now))
            .map(SessionDto::from)
            .collect();
        Ok(roster
            .iter()
            .filter(|a| !cmd.live_only || present.contains(a.id()))
            .map(|actor| {
                let mut dto = ActorDto::from(actor);
                dto.sessions = live.iter().filter(|s| s.actor == dto.id).cloned().collect();
                dto
            })
            .collect())
    }
}
