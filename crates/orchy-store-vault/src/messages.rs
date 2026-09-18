use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{ActorId, ActorStore, EntityKind, EventLog, Id, Message, MessageStore, Result};

use crate::codec;
use crate::vault::Vault;

pub struct VaultMessageStore {
    vault: Arc<Vault>,
    actors: Arc<dyn ActorStore>,
    log: Arc<dyn EventLog>,
}

impl VaultMessageStore {
    pub fn new(vault: Arc<Vault>, actors: Arc<dyn ActorStore>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, actors, log }
    }

    async fn all(&self) -> Result<Vec<Message>> {
        let mut messages = Vec::new();
        for (key, file) in self.vault.load_all(EntityKind::Message).await? {
            messages.push(codec::message_from_markdown(&file, &key)?);
        }
        messages.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(messages)
    }
}

#[async_trait]
impl MessageStore for VaultMessageStore {
    async fn get(&self, id: &Id) -> Result<Option<Message>> {
        let Some((key, file)) = self.vault.read_by_id(id).await? else {
            return Ok(None);
        };
        if codec::kind_of(&file) != Some("message") {
            return Ok(None);
        }
        codec::message_from_markdown(&file, &key).map(Some)
    }

    async fn thread(&self, thread: &Id) -> Result<Vec<Message>> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|m| m.thread() == thread)
            .collect())
    }

    async fn inbox(&self, for_actor: &ActorId, after: Option<&Id>) -> Result<Vec<Message>> {
        let Some(actor) = self.actors.get(for_actor).await? else {
            return Ok(Vec::new());
        };
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|m| m.from() != for_actor)
            .filter(|m| m.is_unread_for(after))
            .filter(|m| m.to().iter().any(|r| r.delivers_to(&actor, m.from())))
            .collect())
    }

    async fn sent_by(&self, actor: &ActorId) -> Result<Vec<Message>> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|m| m.from() == actor)
            .collect())
    }

    async fn save(&self, message: &mut Message) -> Result<()> {
        let events = message.drain_events();
        let key = self
            .vault
            .layout()
            .message_key(message.thread(), message.id());
        let file = codec::message_to_markdown(message);
        self.vault
            .write(&key, &file, message.id(), EntityKind::Message)
            .await?;
        self.log.append(&events).await
    }
}
