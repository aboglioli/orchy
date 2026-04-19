pub mod ackers;

use std::future::Future;

use async_trait::async_trait;

use crate::error::Result;
use crate::event::Event;

pub trait Acker: Send + Sync + Clone {
    fn ack(&self) -> impl Future<Output = Result<()>> + Send;
    fn nack(&self) -> impl Future<Output = Result<()>> + Send;
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
