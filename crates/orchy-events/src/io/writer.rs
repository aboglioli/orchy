use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;
use crate::event::Event;

pub type BoxWriter = Box<dyn Writer + Send + Sync>;
pub type ArcWriter = Arc<dyn Writer + Send + Sync>;

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
impl<T: Writer + ?Sized> Writer for Arc<T> {
    async fn write(&self, event: &Event) -> Result<()> {
        (**self).write(event).await
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        (**self).write_all(events).await
    }
}

#[async_trait]
impl<T: Writer + ?Sized> Writer for Box<T> {
    async fn write(&self, event: &Event) -> Result<()> {
        (**self).write(event).await
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        (**self).write_all(events).await
    }
}

pub trait WriterExt: Writer + Sized + 'static {
    fn into_boxed(self) -> BoxWriter {
        Box::new(self)
    }

    fn into_arced(self) -> ArcWriter {
        Arc::new(self)
    }
}

impl<T: Writer + 'static> WriterExt for T {}
