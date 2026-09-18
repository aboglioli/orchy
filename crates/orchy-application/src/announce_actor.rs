use std::sync::Arc;

use orchy_core::{Actor, ActorId, ActorStore, Clock, Namespace, Role};
use serde::{Deserialize, Serialize};

use crate::dto::ActorDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnounceActorCommand {
    pub actor: String,
    pub roles: Vec<String>,
    pub namespace: Option<String>,
    pub display_name: Option<String>,
}

pub struct AnnounceActor {
    actors: Arc<dyn ActorStore>,
    clock: Arc<dyn Clock>,
}

impl AnnounceActor {
    pub fn new(actors: Arc<dyn ActorStore>, clock: Arc<dyn Clock>) -> Self {
        Self { actors, clock }
    }

    pub async fn execute(&self, cmd: AnnounceActorCommand) -> ApplicationResult<ActorDto> {
        let id: ActorId = cmd.actor.parse()?;
        let roles = cmd
            .roles
            .iter()
            .map(Role::new)
            .collect::<orchy_core::Result<Vec<_>>>()?;
        let namespace = match &cmd.namespace {
            Some(ns) => Namespace::new(ns)?,
            None => Namespace::root(),
        };

        let mut actor = match self.actors.get(&id).await? {
            Some(mut existing) => {
                if !roles.is_empty() {
                    existing.set_roles(roles);
                }
                existing.move_to(namespace);
                existing.seen_at(self.clock.now());
                existing
            }
            None => Actor::announce(id, roles, namespace, &*self.clock),
        };

        if cmd.display_name.is_some() {
            actor.rename(cmd.display_name);
        }

        self.actors.save(&mut actor).await?;
        Ok(ActorDto::from(&actor))
    }
}
