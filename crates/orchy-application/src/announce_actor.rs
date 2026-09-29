use std::sync::Arc;

use orchy_core::{Actor, ActorId, ActorStore, Clock, Namespace, Role};
use serde::{Deserialize, Serialize};

use crate::brief::{Brief, BriefCommand};
use crate::dto::BriefingDto;
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
    brief: Arc<Brief>,
    clock: Arc<dyn Clock>,
}

impl AnnounceActor {
    pub fn new(actors: Arc<dyn ActorStore>, brief: Arc<Brief>, clock: Arc<dyn Clock>) -> Self {
        Self {
            actors,
            brief,
            clock,
        }
    }

    pub async fn execute(&self, cmd: AnnounceActorCommand) -> ApplicationResult<BriefingDto> {
        let id: ActorId = cmd.actor.parse()?;
        let roles = cmd
            .roles
            .iter()
            .map(Role::new)
            .collect::<orchy_core::Result<Vec<_>>>()?;
        let namespace = cmd.namespace.as_deref().map(Namespace::new).transpose()?;

        let existing = self.actors.get(&id).await?;
        let last_seen = existing.as_ref().map(|actor| actor.last_seen());
        let mut actor = match existing {
            Some(mut existing) => {
                if !roles.is_empty() {
                    existing.set_roles(roles, &*self.clock);
                }
                if let Some(namespace) = namespace {
                    existing.move_to(namespace, &*self.clock);
                }
                existing.seen_at(self.clock.now());
                existing
            }
            None => Actor::announce(id, roles, namespace.unwrap_or_default(), &*self.clock),
        };

        if cmd.display_name.is_some() {
            actor.rename(cmd.display_name, &*self.clock);
        }

        self.actors.save(&mut actor).await?;
        self.brief
            .execute(BriefCommand {
                actor: cmd.actor,
                since: last_seen,
            })
            .await
    }
}
