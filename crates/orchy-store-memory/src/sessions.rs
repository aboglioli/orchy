use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use orchy_core::{EventLog, Result, Session, SessionStore, SessionToken};

pub struct MemorySessionStore {
    sessions: Mutex<BTreeMap<SessionToken, Session>>,
    log: Arc<dyn EventLog>,
}

impl MemorySessionStore {
    pub fn new(log: Arc<dyn EventLog>) -> Self {
        Self {
            sessions: Mutex::new(BTreeMap::new()),
            log,
        }
    }
}

#[async_trait]
impl SessionStore for MemorySessionStore {
    async fn get(&self, token: &SessionToken) -> Result<Option<Session>> {
        Ok(self
            .sessions
            .lock()
            .expect("sessions lock")
            .get(token)
            .cloned())
    }

    async fn save(&self, session: &mut Session) -> Result<()> {
        let events = session.drain_events();
        self.sessions
            .lock()
            .expect("sessions lock")
            .insert(session.token().clone(), session.clone());
        self.log.append(&events).await
    }

    async fn all(&self) -> Result<Vec<Session>> {
        Ok(self
            .sessions
            .lock()
            .expect("sessions lock")
            .values()
            .cloned()
            .collect())
    }

    async fn touch(&self, token: &SessionToken, now: DateTime<Utc>) -> Result<()> {
        if let Some(session) = self.sessions.lock().expect("sessions lock").get_mut(token) {
            session.seen_at(now);
        }
        Ok(())
    }
}
