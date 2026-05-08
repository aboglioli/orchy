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

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingAcker {
        acks: Arc<AtomicUsize>,
        nacks: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl Acker for CountingAcker {
        async fn ack(&self) -> Result<()> {
            self.acks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn nack(&self) -> Result<()> {
            self.nacks.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    fn counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
    }

    #[tokio::test]
    async fn into_boxed_yields_dyn_acker() {
        let (acks, nacks) = counters();
        let acker: BoxAcker = CountingAcker {
            acks: Arc::clone(&acks),
            nacks: Arc::clone(&nacks),
        }
        .into_boxed();
        acker.ack().await.unwrap();
        acker.nack().await.unwrap();
        assert_eq!(acks.load(Ordering::SeqCst), 1);
        assert_eq!(nacks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn into_arced_yields_shared_acker() {
        let (acks, _) = counters();
        let acker: ArcAcker = CountingAcker {
            acks: Arc::clone(&acks),
            nacks: Arc::new(AtomicUsize::new(0)),
        }
        .into_arced();
        let clone = Arc::clone(&acker);
        acker.ack().await.unwrap();
        clone.ack().await.unwrap();
        assert_eq!(acks.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn box_blanket_passes_as_generic_acker() {
        async fn take<A: Acker>(a: A) {
            a.ack().await.unwrap();
        }
        let (acks, nacks) = counters();
        let boxed: BoxAcker = CountingAcker {
            acks: Arc::clone(&acks),
            nacks,
        }
        .into_boxed();
        take(boxed).await;
        assert_eq!(acks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn arc_blanket_passes_as_generic_acker() {
        async fn take<A: Acker>(a: A) {
            a.ack().await.unwrap();
        }
        let (acks, nacks) = counters();
        let arced: ArcAcker = CountingAcker {
            acks: Arc::clone(&acks),
            nacks,
        }
        .into_arced();
        take(arced).await;
        assert_eq!(acks.load(Ordering::SeqCst), 1);
    }
}
