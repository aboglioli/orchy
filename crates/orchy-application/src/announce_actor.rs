use std::sync::Arc;

use orchy_core::{
    Actor, ActorId, ActorStore, Clock, IdGenerator, Namespace, Role, Session, SessionStore,
    SessionToken, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::brief::{Brief, BriefCommand};
use crate::dto::{BriefingDto, SessionDto};
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AnnounceActorCommand {
    pub actor: String,
    pub roles: Vec<String>,
    pub namespace: Option<String>,
    pub display_name: Option<String>,
    pub session: Option<String>,
}

pub struct AnnounceActorSources {
    pub actors: Arc<dyn ActorStore>,
    pub sessions: Arc<dyn SessionStore>,
    pub brief: Arc<Brief>,
    pub ids: Arc<dyn IdGenerator>,
    pub clock: Arc<dyn Clock>,
    pub unit_of_work: Arc<dyn UnitOfWork>,
}

pub struct AnnounceActor {
    actors: Arc<dyn ActorStore>,
    sessions: Arc<dyn SessionStore>,
    brief: Arc<Brief>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl AnnounceActor {
    pub fn new(sources: AnnounceActorSources) -> Self {
        let AnnounceActorSources {
            actors,
            sessions,
            brief,
            ids,
            clock,
            unit_of_work,
        } = sources;
        Self {
            actors,
            sessions,
            brief,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: AnnounceActorCommand) -> ApplicationResult<BriefingDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: AnnounceActorCommand) -> ApplicationResult<BriefingDto> {
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
                    existing.set_roles(roles.clone(), &*self.clock);
                }
                if let Some(namespace) = namespace.clone() {
                    existing.move_to(namespace, &*self.clock);
                }
                existing.seen_at(self.clock.now());
                existing
            }
            None => Actor::announce(
                id.clone(),
                roles.clone(),
                namespace.clone().unwrap_or_default(),
                &*self.clock,
            ),
        };

        if cmd.display_name.is_some() {
            actor.rename(cmd.display_name, &*self.clock);
        }
        self.actors.save(&mut actor).await?;

        let mut session = match self.resumable(cmd.session.as_deref(), &id).await? {
            Some(mut session) => {
                session.resume(roles, namespace, &*self.clock);
                session
            }
            None => Session::start(
                id,
                actor.roles().to_vec(),
                actor.namespace().clone(),
                &*self.ids,
                &*self.clock,
            ),
        };
        self.sessions.save(&mut session).await?;

        let mut briefing = self
            .brief
            .execute(BriefCommand {
                actor: cmd.actor,
                since: last_seen,
            })
            .await?;
        briefing.session = Some(SessionDto::from(&session));
        Ok(briefing)
    }

    async fn resumable(
        &self,
        token: Option<&str>,
        actor: &ActorId,
    ) -> ApplicationResult<Option<Session>> {
        let Some(token) = token.and_then(|t| t.parse::<SessionToken>().ok()) else {
            return Ok(None);
        };
        Ok(self
            .sessions
            .get(&token)
            .await?
            .filter(|s| s.is_live(self.clock.now()) && s.actor() == actor))
    }
}
