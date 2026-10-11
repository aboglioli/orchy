use std::fmt;
use std::str::FromStr;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use eventuary::{Payload, Topic};
use serde::{Deserialize, Serialize};

use super::Role;
use super::identity::ActorId;
use crate::clock::Clock;
use crate::error::{DomainError, Result};
use crate::event::{DomainEvent, EventCollector, payload_of, topic};
use crate::id::{Id, IdGenerator};
use crate::namespace::Namespace;

const PREFIX: &str = "ses_";
pub const SESSION_IDLE_DAYS: i64 = 7;

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn get(&self, token: &SessionToken) -> Result<Option<Session>>;
    async fn save(&self, session: &mut Session) -> Result<()>;
    async fn all(&self) -> Result<Vec<Session>>;
    async fn touch(&self, token: &SessionToken, now: DateTime<Utc>) -> Result<()>;

    async fn require_live(&self, token: &SessionToken, now: DateTime<Utc>) -> Result<Session> {
        match self.get(token).await? {
            Some(session) if session.is_live(now) => Ok(session),
            Some(_) => Err(DomainError::not_found(
                "session",
                format!("{token} (ended)"),
            )),
            None => Err(DomainError::not_found("session", token)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionToken(Id);

impl SessionToken {
    pub fn new(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref().trim();
        let id = value.strip_prefix(PREFIX).ok_or_else(|| {
            DomainError::validation(format!(
                "`{value}` is not a session token; `orchy announce` gives one starting with `{PREFIX}`"
            ))
        })?;
        Ok(Self(Id::new(id.to_uppercase())?))
    }

    pub fn generate(ids: &dyn IdGenerator) -> Self {
        Self(Id::generate(ids))
    }

    pub fn id(&self) -> &Id {
        &self.0
    }
}

impl fmt::Display for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{PREFIX}{}", self.0.to_string().to_lowercase())
    }
}

impl FromStr for SessionToken {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self> {
        Self::new(s)
    }
}

impl TryFrom<String> for SessionToken {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self> {
        Self::new(value)
    }
}

impl From<SessionToken> for String {
    fn from(token: SessionToken) -> Self {
        token.to_string()
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    token: SessionToken,
    actor: ActorId,
    roles: Vec<Role>,
    namespace: Namespace,
    started_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
    collector: EventCollector,
}

#[derive(Debug, Clone)]
pub struct RestoreSession {
    pub token: SessionToken,
    pub actor: ActorId,
    pub roles: Vec<Role>,
    pub namespace: Namespace,
    pub started_at: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl Session {
    pub fn new(restore: RestoreSession) -> Self {
        Self {
            token: restore.token,
            actor: restore.actor,
            roles: restore.roles,
            namespace: restore.namespace,
            started_at: restore.started_at,
            last_seen: restore.last_seen,
            ended_at: restore.ended_at,
            collector: EventCollector::new(),
        }
    }

    pub fn start(
        actor: ActorId,
        roles: Vec<Role>,
        namespace: Namespace,
        ids: &dyn IdGenerator,
        clock: &dyn Clock,
    ) -> Self {
        let now = clock.now();
        let mut session = Self::new(RestoreSession {
            token: SessionToken::generate(ids),
            actor,
            roles,
            namespace,
            started_at: now,
            last_seen: now,
            ended_at: None,
        });
        session.collector.collect(SessionStarted {
            token: session.token.clone(),
            actor: session.actor.clone(),
            namespace: session.namespace.clone(),
            roles: session.roles.clone(),
            at: now,
        });
        session
    }

    pub fn ensure_held_by(&self, actor: &ActorId) -> Result<()> {
        if &self.actor == actor {
            return Ok(());
        }
        Err(DomainError::forbidden(format!(
            "session {} belongs to {}, not {actor}",
            self.token, self.actor
        )))
    }

    pub fn resume(&mut self, roles: Vec<Role>, namespace: Option<Namespace>, clock: &dyn Clock) {
        if !roles.is_empty() {
            self.roles = roles;
        }
        if let Some(namespace) = namespace {
            self.namespace = namespace;
        }
        self.last_seen = clock.now();
        self.collector.collect(SessionResumed {
            token: self.token.clone(),
            actor: self.actor.clone(),
            namespace: self.namespace.clone(),
            roles: self.roles.clone(),
            at: self.last_seen,
        });
    }

    pub fn seen_at(&mut self, now: DateTime<Utc>) {
        if self.ended_at.is_none() && now > self.last_seen {
            self.last_seen = now;
        }
    }

    pub fn end(&mut self, clock: &dyn Clock) -> Result<()> {
        if self.ended_at.is_some() {
            return Err(DomainError::conflict(format!(
                "session {} has already ended",
                self.token
            )));
        }
        let now = clock.now();
        self.ended_at = Some(now);
        self.collector.collect(SessionEnded {
            token: self.token.clone(),
            actor: self.actor.clone(),
            namespace: self.namespace.clone(),
            at: now,
        });
        Ok(())
    }

    pub fn is_live(&self, now: DateTime<Utc>) -> bool {
        self.ended_at.is_none() && now - self.last_seen < Duration::days(SESSION_IDLE_DAYS)
    }

    pub fn drain_events(&mut self) -> Vec<Box<dyn DomainEvent>> {
        self.collector.drain()
    }

    pub fn token(&self) -> &SessionToken {
        &self.token
    }
    pub fn actor(&self) -> &ActorId {
        &self.actor
    }
    pub fn roles(&self) -> &[Role] {
        &self.roles
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }
    pub fn last_seen(&self) -> DateTime<Utc> {
        self.last_seen
    }
    pub fn ended_at(&self) -> Option<DateTime<Utc>> {
        self.ended_at
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStarted {
    pub token: SessionToken,
    pub actor: ActorId,
    pub namespace: Namespace,
    pub roles: Vec<Role>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResumed {
    pub token: SessionToken,
    pub actor: ActorId,
    pub namespace: Namespace,
    pub roles: Vec<Role>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEnded {
    pub token: SessionToken,
    pub actor: ActorId,
    pub namespace: Namespace,
    pub at: DateTime<Utc>,
}

macro_rules! session_event {
    ($event:ident, $topic:literal) => {
        impl DomainEvent for $event {
            fn topic(&self) -> Topic {
                topic($topic)
            }
            fn key(&self) -> Id {
                self.token.id().clone()
            }
            fn namespace(&self) -> Namespace {
                self.namespace.clone()
            }
            fn payload(&self) -> Result<Payload> {
                payload_of(self)
            }
        }
    };
}

session_event!(SessionStarted, "session.started");
session_event!(SessionResumed, "session.resumed");
session_event!(SessionEnded, "session.ended");

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use ulid::Ulid;

    use super::*;

    const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    struct FixedClock(DateTime<Utc>);

    impl Clock for FixedClock {
        fn now(&self) -> DateTime<Utc> {
            self.0
        }
    }

    struct SeqIds(AtomicU64);

    impl IdGenerator for SeqIds {
        fn generate(&self) -> Ulid {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            Ulid::from_parts(1_700_000_000_000 + n, u128::from(n))
        }
    }

    fn at(secs: i64) -> FixedClock {
        FixedClock(DateTime::from_timestamp(secs, 0).unwrap())
    }

    fn actor(alias: &str) -> ActorId {
        ActorId::new(alias, MACHINE).unwrap()
    }

    fn started() -> Session {
        Session::start(
            actor("coder-1"),
            vec![Role::new("developer").unwrap()],
            Namespace::new("/backend").unwrap(),
            &SeqIds(AtomicU64::new(1)),
            &at(1_700_000_000),
        )
    }

    #[test]
    fn a_token_reads_back_as_written_and_names_itself() {
        let session = started();
        let written = session.token().to_string();
        assert!(written.starts_with("ses_"), "{written}");
        assert_eq!(written.parse::<SessionToken>().unwrap(), *session.token());
        assert!(
            "01ARZ3NDEKTSV4RRFFQ69G5FAV"
                .parse::<SessionToken>()
                .is_err()
        );
        assert!("ses_nope".parse::<SessionToken>().is_err());
    }

    #[test]
    fn a_session_lives_until_it_ends_or_goes_idle() {
        let mut session = started();
        assert!(session.is_live(at(1_700_000_000 + 3_600).now()));
        assert!(!session.is_live(at(1_700_000_000 + 8 * 86_400).now()));
        session.end(&at(1_700_000_100)).unwrap();
        assert!(!session.is_live(at(1_700_000_200).now()));
        assert!(session.end(&at(1_700_000_300)).is_err());
    }

    #[test]
    fn resuming_keeps_what_is_not_given_again() {
        let mut session = started();
        session.resume(Vec::new(), None, &at(1_700_000_500));
        assert_eq!(session.namespace().as_str(), "/backend");
        assert_eq!(session.roles().len(), 1);
        assert_eq!(session.last_seen(), at(1_700_000_500).now());
    }

    #[test]
    fn a_session_belongs_to_the_agent_that_announced_it() {
        let session = started();
        assert!(session.ensure_held_by(&actor("coder-1")).is_ok());
        assert!(session.ensure_held_by(&actor("coder-2")).is_err());
    }

    #[test]
    fn every_change_to_a_session_records_an_event() {
        let mut session = started();
        assert_eq!(session.drain_events().len(), 1);
        session.resume(Vec::new(), None, &at(1_700_000_500));
        assert_eq!(session.drain_events().len(), 1);
        session.end(&at(1_700_000_600)).unwrap();
        let ended = session.drain_events();
        assert_eq!(ended[0].topic().as_str(), "session.ended");
        assert_eq!(&ended[0].key(), session.token().id());
    }
}
