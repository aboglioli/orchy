use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::Stream;
use tokio::sync::{RwLock, mpsc};
use tokio_util::sync::CancellationToken;

use orchy_events::io::acker::{Either, NoopAcker};
use orchy_events::io::{Acker, Message, Reader};
use orchy_events::{
    ConsumerGroupId, Error, Namespace, OrganizationId, Result, SerializedEvent, StartFrom, Topic,
};

use crate::MemoryState;

#[derive(Clone)]
pub struct MemoryReaderConfig {
    pub organization: OrganizationId,
    pub consumer_group_id: Option<ConsumerGroupId>,
    pub start_from: StartFrom,
    pub topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

#[derive(Clone)]
pub struct MemoryAcker {
    offsets: Arc<RwLock<HashMap<ConsumerGroupId, usize>>>,
    group_id: ConsumerGroupId,
    next_offset: usize,
}

#[async_trait]
impl Acker for MemoryAcker {
    async fn ack(&self) -> Result<()> {
        let mut g = self.offsets.write().await;
        let entry = g.entry(self.group_id.clone()).or_insert(0);
        if self.next_offset > *entry {
            *entry = self.next_offset;
        }
        Ok(())
    }

    async fn nack(&self) -> Result<()> {
        Ok(())
    }
}

pub type MemoryAckerVariant = Either<MemoryAcker, NoopAcker>;

pub struct MemoryStream {
    rx: mpsc::Receiver<Result<Message<MemoryAckerVariant>>>,
    cancel: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl Drop for MemoryStream {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }
}

impl Stream for MemoryStream {
    type Item = Result<Message<MemoryAckerVariant>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub struct MemoryReader {
    state: Arc<MemoryState>,
    offsets: Arc<RwLock<HashMap<ConsumerGroupId, usize>>>,
    config: MemoryReaderConfig,
}

impl MemoryReader {
    pub fn new(
        state: Arc<MemoryState>,
        offsets: Arc<RwLock<HashMap<ConsumerGroupId, usize>>>,
        config: MemoryReaderConfig,
    ) -> Self {
        Self {
            state,
            offsets,
            config,
        }
    }
}

fn matches_filters(serialized: &SerializedEvent, config: &MemoryReaderConfig) -> bool {
    if serialized.organization != config.organization.as_str() {
        return false;
    }
    if let Some(ts) = config.end_at
        && serialized.timestamp > ts
    {
        return false;
    }
    if let Some(topics) = config.topics.as_ref() {
        if !topics.iter().any(|t| t.as_str() == serialized.topic) {
            return false;
        }
    }
    if let Some(prefix) = config.namespace_prefix.as_ref()
        && !prefix.is_root()
    {
        let ns = &serialized.namespace;
        let p = prefix.as_str();
        if ns != p && !ns.starts_with(&format!("{p}/")) {
            return false;
        }
    }
    true
}

fn matches_start_position(
    serialized: &SerializedEvent,
    start_from: StartFrom,
    initial_len: usize,
    idx: usize,
) -> bool {
    match start_from {
        StartFrom::Earliest => true,
        StartFrom::Latest => idx >= initial_len,
        StartFrom::Timestamp(ts) => serialized.timestamp >= ts,
    }
}

#[async_trait]
impl Reader for MemoryReader {
    type Acker = MemoryAckerVariant;
    type Stream = MemoryStream;

    async fn read(&self) -> Result<Self::Stream> {
        let state = Arc::clone(&self.state);
        let offsets = Arc::clone(&self.offsets);
        let config = self.config.clone();
        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let initial_len = state.events.read().await.len();

        let starting_index = if let Some(group) = config.consumer_group_id.as_ref() {
            let map = offsets.read().await;
            if let Some(o) = map.get(group) {
                *o
            } else {
                match config.start_from {
                    StartFrom::Earliest => 0,
                    StartFrom::Latest => initial_len,
                    StartFrom::Timestamp(_) => 0,
                }
            }
        } else {
            match config.start_from {
                StartFrom::Earliest => 0,
                StartFrom::Latest => initial_len,
                StartFrom::Timestamp(_) => 0,
            }
        };

        let handle = tokio::spawn(async move {
            let bounded = config.end_at.is_some() || config.limit.is_some();
            let mut idx = starting_index;
            let mut delivered = 0usize;
            loop {
                if cancel_for_task.is_cancelled() {
                    break;
                }
                let notified = state.events_notify.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let snapshot = state.events.read().await.clone();
                while idx < snapshot.len() {
                    let serialized = &snapshot[idx];
                    if matches_filters(serialized, &config)
                        && matches_start_position(serialized, config.start_from, initial_len, idx)
                    {
                        let event = match serialized.to_event() {
                            Ok(e) => e,
                            Err(e) => {
                                let _ = tx.send(Err(Error::Store(e.to_string()))).await;
                                idx += 1;
                                continue;
                            }
                        };
                        let next_offset = idx + 1;
                        let acker = if let Some(group) = config.consumer_group_id.as_ref() {
                            Either::Left(MemoryAcker {
                                offsets: Arc::clone(&offsets),
                                group_id: group.clone(),
                                next_offset,
                            })
                        } else {
                            Either::Right(NoopAcker)
                        };
                        if tx.send(Ok(Message::new(event, acker))).await.is_err() {
                            return;
                        }
                        delivered += 1;
                        if let Some(l) = config.limit
                            && delivered >= l
                        {
                            return;
                        }
                    }
                    idx += 1;
                }
                if bounded {
                    return;
                }
                tokio::select! {
                    _ = notified => {}
                    _ = cancel_for_task.cancelled() => return,
                }
            }
        });

        Ok(MemoryStream {
            rx,
            cancel,
            handle: Some(handle),
        })
    }
}
