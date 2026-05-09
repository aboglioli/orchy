use either::Either;

use crate::error::Result;
use crate::io::Acker;

impl<L: Acker, R: Acker> Acker for Either<L, R> {
    async fn ack(&self) -> Result<()> {
        match self {
            Either::Left(a) => a.ack().await,
            Either::Right(a) => a.ack().await,
        }
    }

    async fn nack(&self) -> Result<()> {
        match self {
            Either::Left(a) => a.nack().await,
            Either::Right(a) => a.nack().await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Counting {
        ack: Arc<AtomicUsize>,
        nack: Arc<AtomicUsize>,
    }

    impl Acker for Counting {
        async fn ack(&self) -> Result<()> {
            self.ack.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        async fn nack(&self) -> Result<()> {
            self.nack.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn left_dispatches_to_left() {
        let ack = Arc::new(AtomicUsize::new(0));
        let nack = Arc::new(AtomicUsize::new(0));
        let acker: Either<Counting, Counting> = Either::Left(Counting {
            ack: Arc::clone(&ack),
            nack: Arc::clone(&nack),
        });
        acker.ack().await.unwrap();
        acker.nack().await.unwrap();
        assert_eq!(ack.load(Ordering::SeqCst), 1);
        assert_eq!(nack.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn right_dispatches_to_right() {
        let ack = Arc::new(AtomicUsize::new(0));
        let nack = Arc::new(AtomicUsize::new(0));
        let acker: Either<Counting, Counting> = Either::Right(Counting {
            ack: Arc::clone(&ack),
            nack: Arc::clone(&nack),
        });
        acker.ack().await.unwrap();
        acker.nack().await.unwrap();
        assert_eq!(ack.load(Ordering::SeqCst), 1);
        assert_eq!(nack.load(Ordering::SeqCst), 1);
    }
}
