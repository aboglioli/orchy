pub mod ackers;

use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::error::Result;
use crate::event::Event;

#[async_trait]
pub trait Acker: Send + Sync + Clone {
    async fn ack(&self) -> Result<()>;
    async fn nack(&self) -> Result<()>;
}

pub struct Message<A: Acker> {
    event: Event,
    acker: A,
}

impl<A: Acker> Message<A> {
    pub fn new(event: Event, acker: A) -> Self {
        Self { event, acker }
    }

    pub fn event(&self) -> &Event {
        &self.event
    }

    pub fn into_event(self) -> Event {
        self.event
    }

    pub async fn ack(&self) -> Result<()> {
        self.acker.ack().await
    }

    pub async fn nack(&self) -> Result<()> {
        self.acker.nack().await
    }

    pub fn acker(&self) -> &A {
        &self.acker
    }
}

#[async_trait]
pub trait Handler: Send + Sync {
    type Acker: Acker;

    fn id(&self) -> &str;
    async fn handle(&self, message: Message<Self::Acker>) -> Result<()>;
}

pub trait Reader: Send + Sync {
    type Acker: Acker;

    #[allow(clippy::type_complexity)]
    fn read(&self) -> Result<Pin<Box<dyn Stream<Item = Result<Message<Self::Acker>>> + Send>>>;
}

#[async_trait]
pub trait Writer: Send + Sync {
    async fn write(&self, event: &Event) -> Result<()>;

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        for event in events {
            self.write(event).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<T: Writer + ?Sized> Writer for std::sync::Arc<T> {
    async fn write(&self, event: &Event) -> Result<()> {
        (**self).write(event).await
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        (**self).write_all(events).await
    }
}
