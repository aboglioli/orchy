#![allow(clippy::needless_maybe_sized)]

pub mod ackers;
pub mod consumers;
pub mod filters;
mod handler;
mod message;
mod reader;
mod writer;

use async_trait::async_trait;

use crate::error::Result;

#[async_trait]
pub trait Acker: Send + Sync {
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

#[async_trait]
impl<T: Acker + ?Sized> Acker for Box<T> {
    async fn ack(&self) -> Result<()> {
        (**self).ack().await
    }
    async fn nack(&self) -> Result<()> {
        (**self).nack().await
    }
}

pub use consumers::{BackgroundConsumer, ConsumerHandle};
pub use handler::{Filter, FilteredHandler, Handler};
pub use message::Message;
pub use reader::{BoxAcker, BoxReader, BoxStream, Reader, ReaderExt};
pub use writer::Writer;
