use std::sync::Arc;

use orchy_core::{ActorStore, Clock};
use serde::{Deserialize, Serialize};

use crate::dto::ActorDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListActorsCommand {
    pub live_only: bool,
}

pub struct ListActors {
    actors: Arc<dyn ActorStore>,
    clock: Arc<dyn Clock>,
}

impl ListActors {
    pub fn new(actors: Arc<dyn ActorStore>, clock: Arc<dyn Clock>) -> Self {
        Self { actors, clock }
    }

    pub async fn execute(&self, cmd: ListActorsCommand) -> ApplicationResult<Vec<ActorDto>> {
        let roster = self.actors.roster().await?;
        if !cmd.live_only {
            return Ok(roster.iter().map(ActorDto::from).collect());
        }
        let present = self.actors.present(self.clock.now()).await?;
        Ok(roster
            .iter()
            .filter(|a| present.contains(a.id()))
            .map(ActorDto::from)
            .collect())
    }
}
