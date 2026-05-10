mod batched;
mod either;
mod noop;
mod once;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::Result;

pub use ::either::Either;
pub use batched::{AckBuffer, AckBufferConfig, BatchFlusher, BatchedAcker};
pub use noop::NoopAcker;
pub use once::OnceAcker;

/// Acknowledges or rejects a delivered event message.
///
/// Backends define exact durability semantics. In general, [`Acker::ack`] marks
/// the message as successfully processed; [`Acker::nack`] requests redelivery
/// when the backend supports it, or leaves the checkpoint unchanged otherwise.
///
/// See per-backend `Acker` implementations for the concrete behavior.
pub trait Acker: Send + Sync {
    /// Marks the message as successfully processed. Concrete durability
    /// (offset commit, row update, queue delete) depends on the backend.
    fn ack(&self) -> impl Future<Output = Result<()>> + Send;
    /// Signals processing failure. Some backends redeliver immediately
    /// (e.g. SQS resets visibility); others leave the checkpoint unchanged
    /// and rely on rebalance/restart for redelivery.
    fn nack(&self) -> impl Future<Output = Result<()>> + Send;
}

impl<T: Acker + ?Sized> Acker for Arc<T> {
    fn ack(&self) -> impl Future<Output = Result<()>> + Send {
        (**self).ack()
    }
    fn nack(&self) -> impl Future<Output = Result<()>> + Send {
        (**self).nack()
    }
}

impl<T: Acker + ?Sized> Acker for Box<T> {
    fn ack(&self) -> impl Future<Output = Result<()>> + Send {
        (**self).ack()
    }
    fn nack(&self) -> impl Future<Output = Result<()>> + Send {
        (**self).nack()
    }
}

pub trait DynAcker: Send + Sync {
    fn ack_dyn<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
    fn nack_dyn<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

impl<T: Acker + ?Sized> DynAcker for T {
    fn ack_dyn<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(<Self as Acker>::ack(self))
    }
    fn nack_dyn<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(<Self as Acker>::nack(self))
    }
}

pub type BoxAcker = Box<dyn DynAcker>;
pub type ArcAcker = Arc<dyn DynAcker>;

impl Acker for dyn DynAcker + '_ {
    fn ack(&self) -> impl Future<Output = Result<()>> + Send {
        DynAcker::ack_dyn(self)
    }
    fn nack(&self) -> impl Future<Output = Result<()>> + Send {
        DynAcker::nack_dyn(self)
    }
}

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
