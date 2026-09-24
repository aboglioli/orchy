use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{Actor, ActorId, EventLog, Id, Message, MessageStore, ReadWatermarks, Result};

use crate::eventlog::MemoryEventLog;

pub struct MemoryMessageStore {
    messages: Mutex<BTreeMap<Id, Message>>,
    actors: Mutex<Vec<Actor>>,
    log: Arc<MemoryEventLog>,
}

impl MemoryMessageStore {
    pub fn new(log: Arc<MemoryEventLog>) -> Self {
        Self {
            messages: Mutex::new(BTreeMap::new()),
            actors: Mutex::new(Vec::new()),
            log,
        }
    }

    pub fn register(&self, actor: Actor) {
        let mut actors = self.actors.lock().expect("actor mutex");
        actors.retain(|a| a.id() != actor.id());
        actors.push(actor);
    }

    fn resolve(&self, id: &ActorId) -> Option<Actor> {
        self.actors
            .lock()
            .expect("actor mutex")
            .iter()
            .find(|a| a.id() == id)
            .cloned()
    }
}

#[async_trait]
impl MessageStore for MemoryMessageStore {
    async fn get(&self, id: &Id) -> Result<Option<Message>> {
        Ok(self
            .messages
            .lock()
            .expect("message mutex")
            .get(id)
            .cloned())
    }

    async fn thread(&self, thread: &Id) -> Result<Vec<Message>> {
        let messages = self.messages.lock().expect("message mutex");
        let mut found: Vec<Message> = messages
            .values()
            .filter(|m| m.thread() == thread)
            .cloned()
            .collect();
        found.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(found)
    }

    async fn inbox(&self, for_actor: &ActorId, after: Option<&Id>) -> Result<Vec<Message>> {
        let Some(actor) = self.resolve(for_actor) else {
            return Ok(Vec::new());
        };
        let messages = self.messages.lock().expect("message mutex");
        let mut found: Vec<Message> = messages
            .values()
            .filter(|m| m.from() != for_actor)
            .filter(|m| m.is_unread_for(after))
            .filter(|m| m.to().iter().any(|r| r.delivers_to(&actor, m.from())))
            .cloned()
            .collect();
        found.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(found)
    }

    async fn sent_by(&self, actor: &ActorId) -> Result<Vec<Message>> {
        let messages = self.messages.lock().expect("message mutex");
        let mut found: Vec<Message> = messages
            .values()
            .filter(|m| m.from() == actor)
            .cloned()
            .collect();
        found.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(found)
    }

    async fn save(&self, message: &mut Message) -> Result<()> {
        let events = message.drain_events();
        self.messages
            .lock()
            .expect("message mutex")
            .insert(message.id().clone(), message.clone());
        self.log.append(&events).await
    }
}

#[derive(Default)]
pub struct MemoryWatermarks(Mutex<HashMap<ActorId, Id>>);

impl MemoryWatermarks {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ReadWatermarks for MemoryWatermarks {
    fn watermark(&self, actor: &ActorId) -> Result<Option<Id>> {
        Ok(self.0.lock().expect("watermark mutex").get(actor).cloned())
    }

    fn advance(&self, actor: &ActorId, to: &Id) -> Result<()> {
        self.0
            .lock()
            .expect("watermark mutex")
            .insert(actor.clone(), to.clone());
        Ok(())
    }
}
