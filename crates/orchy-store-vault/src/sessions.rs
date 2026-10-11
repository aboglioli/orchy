use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orchy_core::{
    ActorId, DomainError, EventLog, Namespace, RestoreSession, Result, Role, Session, SessionStore,
    SessionToken,
};
use serde::{Deserialize, Serialize};

use crate::lock::{DEFAULT_WAIT, FileLock};

pub struct FileSessionStore {
    root: PathBuf,
    log: Arc<dyn EventLog>,
}

#[derive(Serialize, Deserialize)]
struct SessionRecord {
    token: String,
    actor: String,
    roles: Vec<String>,
    namespace: String,
    started_at: DateTime<Utc>,
    last_seen: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}

impl SessionRecord {
    fn of(session: &Session) -> Self {
        Self {
            token: session.token().to_string(),
            actor: session.actor().to_string(),
            roles: session.roles().iter().map(ToString::to_string).collect(),
            namespace: session.namespace().to_string(),
            started_at: session.started_at(),
            last_seen: session.last_seen(),
            ended_at: session.ended_at(),
        }
    }

    fn into_session(self) -> Result<Session> {
        Ok(Session::new(RestoreSession {
            token: self.token.parse()?,
            actor: self.actor.parse::<ActorId>()?,
            roles: self
                .roles
                .iter()
                .map(Role::new)
                .collect::<Result<Vec<_>>>()?,
            namespace: Namespace::new(&self.namespace)?,
            started_at: self.started_at,
            last_seen: self.last_seen,
            ended_at: self.ended_at,
        }))
    }
}

impl FileSessionStore {
    pub fn new(root: impl Into<PathBuf>, log: Arc<dyn EventLog>) -> Self {
        Self {
            root: root.into(),
            log,
        }
    }

    fn path(&self, token: &SessionToken) -> PathBuf {
        self.root.join(format!("{token}.json"))
    }

    fn read(&self, token: &SessionToken) -> Result<Option<Session>> {
        read_session(&self.root, token)
    }
}

pub fn read_session(root: &Path, token: &SessionToken) -> Result<Option<Session>> {
    let path = root.join(format!("{token}.json"));
    if !path.exists() {
        return Ok(None);
    }
    let lock = FileLock::shared(&path, "session", DEFAULT_WAIT)?;
    read_record(lock.file())?
        .map(SessionRecord::into_session)
        .transpose()
}

fn read_record(file: &File) -> Result<Option<SessionRecord>> {
    let mut handle = file;
    let mut text = String::new();
    handle
        .seek(SeekFrom::Start(0))
        .and_then(|_| handle.read_to_string(&mut text))
        .map_err(|e| DomainError::unavailable(format!("reading session: {e}")))?;
    if text.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| DomainError::validation(format!("session record is unreadable: {e}")))
}

fn write_record(file: &File, record: &SessionRecord) -> Result<()> {
    let mut handle = file;
    let bytes = serde_json::to_vec_pretty(record)
        .map_err(|e| DomainError::unavailable(format!("encoding session: {e}")))?;
    file.set_len(0)
        .and_then(|()| handle.seek(SeekFrom::Start(0)))
        .and_then(|_| handle.write_all(&bytes))
        .and_then(|()| file.sync_all())
        .map_err(|e| DomainError::unavailable(format!("writing session: {e}")))
}

#[async_trait]
impl SessionStore for FileSessionStore {
    async fn get(&self, token: &SessionToken) -> Result<Option<Session>> {
        self.read(token)
    }

    async fn save(&self, session: &mut Session) -> Result<()> {
        let events = session.drain_events();
        {
            let lock = FileLock::exclusive(&self.path(session.token()), "session", DEFAULT_WAIT)?;
            write_record(lock.file(), &SessionRecord::of(session))?;
        }
        self.log.append(&events).await
    }

    async fn all(&self) -> Result<Vec<Session>> {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(Vec::new());
        };
        let mut sessions = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(token) = name
                .strip_suffix(".json")
                .and_then(|t| t.parse::<SessionToken>().ok())
            else {
                continue;
            };
            if let Ok(Some(session)) = self.read(&token) {
                sessions.push(session);
            }
        }
        sessions.sort_by_key(Session::started_at);
        Ok(sessions)
    }

    async fn touch(&self, token: &SessionToken, now: DateTime<Utc>) -> Result<()> {
        let path = self.path(token);
        if !path.exists() {
            return Ok(());
        }
        let lock = FileLock::exclusive(&path, "session", DEFAULT_WAIT)?;
        let Some(mut record) = read_record(lock.file())? else {
            return Ok(());
        };
        if record.ended_at.is_some() || record.last_seen >= now {
            return Ok(());
        }
        record.last_seen = now;
        write_record(lock.file(), &record)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use orchy_core::{Clock, IdGenerator};
    use orchy_store_memory::{FixedClock, MemoryEventLog, SeqIdGenerator};

    use super::*;

    const MACHINE: &str = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

    fn store(root: &std::path::Path) -> FileSessionStore {
        FileSessionStore::new(root.join("sessions"), Arc::new(MemoryEventLog::new()))
    }

    fn start(clock: &dyn Clock, ids: &dyn IdGenerator) -> Session {
        Session::start(
            ActorId::new("coder-1", MACHINE).unwrap(),
            vec![Role::new("developer").unwrap()],
            Namespace::new("/backend").unwrap(),
            ids,
            clock,
        )
    }

    #[tokio::test]
    async fn a_session_reads_back_as_it_was_saved() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = store(temp.path());
        let clock = FixedClock::at(1_700_000_000);
        let mut session = start(&clock, &SeqIdGenerator::new());
        sessions.save(&mut session).await.unwrap();

        let read = sessions.get(session.token()).await.unwrap().unwrap();
        assert_eq!(read.actor(), session.actor());
        assert_eq!(read.namespace().as_str(), "/backend");
        assert_eq!(read.roles(), session.roles());
        assert_eq!(sessions.all().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn touching_moves_last_seen_forward_only() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = store(temp.path());
        let clock = FixedClock::at(1_700_000_000);
        let mut session = start(&clock, &SeqIdGenerator::new());
        sessions.save(&mut session).await.unwrap();

        let later = DateTime::from_timestamp(1_700_000_600, 0).unwrap();
        sessions.touch(session.token(), later).await.unwrap();
        let earlier = DateTime::from_timestamp(1_700_000_100, 0).unwrap();
        sessions.touch(session.token(), earlier).await.unwrap();
        let read = sessions.get(session.token()).await.unwrap().unwrap();
        assert_eq!(read.last_seen(), later);
    }

    #[tokio::test]
    async fn an_unknown_or_ended_session_is_not_live() {
        let temp = tempfile::tempdir().unwrap();
        let sessions = store(temp.path());
        let clock = FixedClock::at(1_700_000_000);
        let mut session = start(&clock, &SeqIdGenerator::new());
        sessions.save(&mut session).await.unwrap();
        session.end(&clock).unwrap();
        sessions.save(&mut session).await.unwrap();

        let ended = sessions.require_live(session.token(), clock.now()).await;
        assert!(
            matches!(ended, Err(DomainError::NotFound { .. })),
            "{ended:?}"
        );
        let unknown: SessionToken = "ses_01arz3ndektsv4rrffq69g5fav".parse().unwrap();
        assert!(sessions.require_live(&unknown, clock.now()).await.is_err());
    }
}
