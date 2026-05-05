#![allow(clippy::needless_maybe_sized)]

pub mod ackers;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;

use crate::error::Result;
use crate::event::Event;
use crate::serialization::SerializedEvent;
use std::result::Result as StdResult;

#[async_trait]
pub trait Acker: Send + Sync + Clone {
    async fn ack(&self) -> Result<()>;
    async fn nack(&self) -> Result<()>;
}

#[async_trait]
impl<T: Acker + ?Sized> Acker for std::sync::Arc<T> {
    async fn ack(&self) -> Result<()> {
        (**self).ack().await
    }
    async fn nack(&self) -> Result<()> {
        (**self).nack().await
    }
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

    pub fn into_parts(self) -> (Event, A) {
        (self.event, self.acker)
    }
}

#[async_trait]
pub trait Handler: Send + Sync {
    fn id(&self) -> &str;
    async fn handle(&self, event: Event) -> Result<()>;
}

#[async_trait]
impl<T: Handler + ?Sized> Handler for std::sync::Arc<T> {
    fn id(&self) -> &str {
        (**self).id()
    }
    async fn handle(&self, event: Event) -> Result<()> {
        (**self).handle(event).await
    }
}

pub trait Filter: Send + Sync {
    fn matches(&self, event: &Event) -> bool;
}

impl<T: Filter + ?Sized> Filter for std::sync::Arc<T> {
    fn matches(&self, event: &Event) -> bool {
        (**self).matches(event)
    }
}

#[async_trait]
pub trait Reader: Send + Sync {
    type Acker: Acker;
    type Stream: Stream<Item = Result<Message<Self::Acker>>> + Send;

    async fn read(&self, consumer_group_id: &str) -> Result<Self::Stream>;
}

#[async_trait]
impl<T: Reader + ?Sized> Reader for std::sync::Arc<T> {
    type Acker = T::Acker;
    type Stream = T::Stream;

    async fn read(&self, consumer_group_id: &str) -> Result<Self::Stream> {
        (**self).read(consumer_group_id).await
    }
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

#[async_trait]
pub trait EventQuery: Send + Sync {
    async fn query_events(
        &self,
        organization: &str,
        since: DateTime<Utc>,
        limit: usize,
    ) -> StdResult<Vec<SerializedEvent>, String>;
}
