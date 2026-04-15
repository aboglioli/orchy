pub mod ackers;

use std::future::Future;

use async_trait::async_trait;
use futures::Stream;

use crate::error::Result;
use crate::event::Event;

pub trait Acker: Send + Sync + Clone {
    fn ack(&self) -> impl Future<Output = Result<()>> + Send;
    fn nack(&self) -> impl Future<Output = Result<()>> + Send;
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

pub trait Handler: Send + Sync {
    type Acker: Acker;

    fn id(&self) -> &str;
    fn handle(&self, message: Message<Self::Acker>) -> impl Future<Output = Result<()>> + Send;
}

pub trait Reader: Send + Sync {
    type Acker: Acker;
    type Stream: Stream<Item = Result<Message<Self::Acker>>> + Send;

    fn read(&self) -> Result<Self::Stream>;
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
