use std::sync::Arc;

use tokio::sync::Mutex;

use crate::error::Result;
use crate::io::Acker;

#[derive(Debug, Clone)]
pub struct OnceAcker<A: Acker> {
    inner: A,
    done: Arc<Mutex<bool>>,
}

impl<A: Acker> OnceAcker<A> {
    pub fn new(inner: A) -> Self {
        Self {
            inner,
            done: Arc::new(Mutex::new(false)),
        }
    }
}

impl<A: Acker> Acker for OnceAcker<A> {
    async fn ack(&self) -> Result<()> {
        let mut done = self.done.lock().await;
        if *done {
            return Ok(());
        }
        *done = true;
        self.inner.ack().await
    }

    async fn nack(&self) -> Result<()> {
        let mut done = self.done.lock().await;
        if *done {
            return Ok(());
        }
        *done = true;
        self.inner.nack().await
    }
}
