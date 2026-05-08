mod batched;
mod either;
mod noop;
mod once;

use std::sync::Arc;

use async_trait::async_trait;

use crate::error::Result;

pub use ::either::Either;
pub use batched::{AckBuffer, AckBufferConfig, BatchFlusher, BatchedAcker};
pub use noop::NoopAcker;
pub use once::OnceAcker;

#[async_trait]
pub trait Acker: Send + Sync {
    async fn ack(&self) -> Result<()>;
    async fn nack(&self) -> Result<()>;
}

#[async_trait]
impl<T: Acker + ?Sized> Acker for Arc<T> {
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

pub type BoxAcker = Box<dyn Acker + Send + Sync>;
pub type ArcAcker = Arc<dyn Acker + Send + Sync>;

pub trait AckerExt: Acker + Sized + 'static {
    fn into_boxed(self) -> BoxAcker {
        Box::new(self)
    }

    fn into_arced(self) -> ArcAcker {
        Arc::new(self)
    }
}

impl<T: Acker + 'static> AckerExt for T {}
