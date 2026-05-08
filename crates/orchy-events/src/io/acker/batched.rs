use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::interval;

use crate::error::{Error, Result};
use crate::io::Acker;

#[async_trait]
pub trait BatchFlusher: Send + Sync {
    type Token: Send + Sync + Clone + 'static;

    async fn flush(&self, acks: Vec<Self::Token>) -> Result<()>;
    async fn flush_nack(&self, nacks: Vec<Self::Token>) -> Result<()>;
}

#[derive(Debug, Clone)]
pub struct AckBufferConfig {
    pub max_pending: usize,
    pub flush_interval: Duration,
}

impl Default for AckBufferConfig {
    fn default() -> Self {
        Self {
            max_pending: 100,
            flush_interval: Duration::from_secs(1),
        }
    }
}

pub enum AckCmd<T> {
    Ack(T),
    Nack(T),
    Flush,
}

pub struct AckBuffer<F: BatchFlusher> {
    tx: mpsc::Sender<AckCmd<F::Token>>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl<F: BatchFlusher + 'static> AckBuffer<F> {
    pub fn spawn(flusher: F, config: AckBufferConfig) -> Arc<Self> {
        let cap = (config.max_pending * 4).max(16);
        let (tx, mut rx) = mpsc::channel(cap);
        let flusher = Arc::new(flusher);
        let handle = tokio::spawn(async move {
            let mut acks: Vec<F::Token> = Vec::with_capacity(config.max_pending);
            let mut nacks: Vec<F::Token> = Vec::with_capacity(config.max_pending);
            let mut tick = interval(config.flush_interval);
            tick.tick().await;
            loop {
                tokio::select! {
                    cmd = rx.recv() => match cmd {
                        Some(AckCmd::Ack(t)) => {
                            acks.push(t);
                            if acks.len() >= config.max_pending {
                                let drained = std::mem::replace(
                                    &mut acks,
                                    Vec::with_capacity(config.max_pending),
                                );
                                if let Err(e) = flusher.flush(drained).await {
                                    tracing::error!("ack buffer flush error: {e}");
                                }
                            }
                        }
                        Some(AckCmd::Nack(t)) => {
                            nacks.push(t);
                            if nacks.len() >= config.max_pending {
                                let drained = std::mem::replace(
                                    &mut nacks,
                                    Vec::with_capacity(config.max_pending),
                                );
                                if let Err(e) = flusher.flush_nack(drained).await {
                                    tracing::error!("ack buffer flush_nack error: {e}");
                                }
                            }
                        }
                        Some(AckCmd::Flush) | None => {
                            if !acks.is_empty() {
                                let drained = std::mem::replace(
                                    &mut acks,
                                    Vec::with_capacity(config.max_pending),
                                );
                                if let Err(e) = flusher.flush(drained).await {
                                    tracing::error!("ack buffer flush error: {e}");
                                }
                            }
                            if !nacks.is_empty() {
                                let drained = std::mem::replace(
                                    &mut nacks,
                                    Vec::with_capacity(config.max_pending),
                                );
                                if let Err(e) = flusher.flush_nack(drained).await {
                                    tracing::error!("ack buffer flush_nack error: {e}");
                                }
                            }
                            break;
                        }
                    },
                    _ = tick.tick() => {
                        if !acks.is_empty() {
                            let drained = std::mem::replace(
                                &mut acks,
                                Vec::with_capacity(config.max_pending),
                            );
                            if let Err(e) = flusher.flush(drained).await {
                                tracing::error!("ack buffer flush error: {e}");
                            }
                        }
                        if !nacks.is_empty() {
                            let drained = std::mem::replace(
                                &mut nacks,
                                Vec::with_capacity(config.max_pending),
                            );
                            if let Err(e) = flusher.flush_nack(drained).await {
                                tracing::error!("ack buffer flush_nack error: {e}");
                            }
                        }
                    }
                }
            }
        });
        Arc::new(Self {
            tx,
            handle: Mutex::new(Some(handle)),
        })
    }

    pub fn sender(&self) -> mpsc::Sender<AckCmd<F::Token>> {
        self.tx.clone()
    }

    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.tx.send(AckCmd::Flush).await;
        drop(self.tx.clone());
        let mut guard = self.handle.lock().await;
        if let Some(h) = guard.take() {
            let _ = h.await;
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct BatchedAcker<T: Send + Sync + Clone + 'static> {
    token: T,
    tx: mpsc::Sender<AckCmd<T>>,
}

impl<T: Send + Sync + Clone + 'static> BatchedAcker<T> {
    pub fn new(token: T, tx: mpsc::Sender<AckCmd<T>>) -> Self {
        Self { token, tx }
    }
}

#[async_trait]
impl<T: Send + Sync + Clone + 'static> Acker for BatchedAcker<T> {
    async fn ack(&self) -> Result<()> {
        self.tx
            .send(AckCmd::Ack(self.token.clone()))
            .await
            .map_err(|e| Error::Store(format!("ack send: {e}")))
    }

    async fn nack(&self) -> Result<()> {
        self.tx
            .send(AckCmd::Nack(self.token.clone()))
            .await
            .map_err(|e| Error::Store(format!("nack send: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};

    struct CountingFlusher {
        ack_calls: Arc<AtomicUsize>,
        ack_total: Arc<AtomicUsize>,
        nack_calls: Arc<AtomicUsize>,
        nack_total: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl BatchFlusher for CountingFlusher {
        type Token = u64;

        async fn flush(&self, acks: Vec<u64>) -> Result<()> {
            self.ack_calls.fetch_add(1, Ordering::SeqCst);
            self.ack_total.fetch_add(acks.len(), Ordering::SeqCst);
            Ok(())
        }

        async fn flush_nack(&self, nacks: Vec<u64>) -> Result<()> {
            self.nack_calls.fetch_add(1, Ordering::SeqCst);
            self.nack_total.fetch_add(nacks.len(), Ordering::SeqCst);
            Ok(())
        }
    }

    #[test]
    fn default_config() {
        let c = AckBufferConfig::default();
        assert_eq!(c.max_pending, 100);
        assert_eq!(c.flush_interval, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn flush_on_max_pending() {
        let ack_calls = Arc::new(AtomicUsize::new(0));
        let ack_total = Arc::new(AtomicUsize::new(0));
        let flusher = CountingFlusher {
            ack_calls: Arc::clone(&ack_calls),
            ack_total: Arc::clone(&ack_total),
            nack_calls: Arc::new(AtomicUsize::new(0)),
            nack_total: Arc::new(AtomicUsize::new(0)),
        };
        let buf = AckBuffer::spawn(
            flusher,
            AckBufferConfig {
                max_pending: 5,
                flush_interval: Duration::from_secs(60),
            },
        );
        let tx = buf.sender();
        for i in 0..5u64 {
            BatchedAcker::new(i, tx.clone()).ack().await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(ack_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ack_total.load(Ordering::SeqCst), 5);
    }

    #[tokio::test]
    async fn flush_on_interval() {
        let ack_calls = Arc::new(AtomicUsize::new(0));
        let ack_total = Arc::new(AtomicUsize::new(0));
        let flusher = CountingFlusher {
            ack_calls: Arc::clone(&ack_calls),
            ack_total: Arc::clone(&ack_total),
            nack_calls: Arc::new(AtomicUsize::new(0)),
            nack_total: Arc::new(AtomicUsize::new(0)),
        };
        let buf = AckBuffer::spawn(
            flusher,
            AckBufferConfig {
                max_pending: 100,
                flush_interval: Duration::from_millis(50),
            },
        );
        let tx = buf.sender();
        BatchedAcker::new(1u64, tx.clone()).ack().await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(ack_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ack_total.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn shutdown_drains_pending() {
        let ack_total = Arc::new(AtomicUsize::new(0));
        let flusher = CountingFlusher {
            ack_calls: Arc::new(AtomicUsize::new(0)),
            ack_total: Arc::clone(&ack_total),
            nack_calls: Arc::new(AtomicUsize::new(0)),
            nack_total: Arc::new(AtomicUsize::new(0)),
        };
        let buf = AckBuffer::spawn(
            flusher,
            AckBufferConfig {
                max_pending: 100,
                flush_interval: Duration::from_secs(60),
            },
        );
        let tx = buf.sender();
        BatchedAcker::new(1u64, tx.clone()).ack().await.unwrap();
        BatchedAcker::new(2u64, tx.clone()).ack().await.unwrap();
        BatchedAcker::new(3u64, tx.clone()).ack().await.unwrap();
        buf.shutdown().await.unwrap();
        assert_eq!(ack_total.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn ack_and_nack_separate_batches() {
        let ack_total = Arc::new(AtomicUsize::new(0));
        let nack_total = Arc::new(AtomicUsize::new(0));
        let flusher = CountingFlusher {
            ack_calls: Arc::new(AtomicUsize::new(0)),
            ack_total: Arc::clone(&ack_total),
            nack_calls: Arc::new(AtomicUsize::new(0)),
            nack_total: Arc::clone(&nack_total),
        };
        let buf = AckBuffer::spawn(
            flusher,
            AckBufferConfig {
                max_pending: 100,
                flush_interval: Duration::from_millis(20),
            },
        );
        let tx = buf.sender();
        BatchedAcker::new(1u64, tx.clone()).ack().await.unwrap();
        BatchedAcker::new(2u64, tx.clone()).nack().await.unwrap();
        BatchedAcker::new(3u64, tx.clone()).ack().await.unwrap();
        tokio::time::sleep(Duration::from_millis(80)).await;
        assert_eq!(ack_total.load(Ordering::SeqCst), 2);
        assert_eq!(nack_total.load(Ordering::SeqCst), 1);
    }
}
