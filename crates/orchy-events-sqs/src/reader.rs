use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use aws_sdk_sqs::Client;
use futures::Stream;
use tokio::sync::{mpsc, watch};

use orchy_events::io::acker::{AckBuffer, BatchedAcker};
use orchy_events::io::{Acker, Message, Reader};
use orchy_events::{Error, Result, SerializedEvent};

use crate::flusher::SqsFlusher;
use crate::reader_config::SqsReaderConfig;

pub struct SqsReader {
    client: Client,
    config: SqsReaderConfig,
}

impl SqsReader {
    pub fn new(client: Client, config: SqsReaderConfig) -> Self {
        Self { client, config }
    }
}

pub struct SqsStream {
    rx: mpsc::Receiver<Result<Message<BatchedAcker<String>>>>,
    shutdown_tx: watch::Sender<bool>,
    handle: Option<tokio::task::JoinHandle<()>>,
    ack_buffer: Arc<AckBuffer<SqsFlusher>>,
}

impl Drop for SqsStream {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(h) = self.handle.take() {
            h.abort();
        }
        let buf = self.ack_buffer.clone();
        tokio::spawn(async move {
            let _ = buf.shutdown().await;
        });
    }
}

impl Stream for SqsStream {
    type Item = Result<Message<BatchedAcker<String>>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

#[async_trait]
impl Reader for SqsReader {
    type Acker = BatchedAcker<String>;
    type Stream = SqsStream;

    async fn read(&self) -> Result<Self::Stream> {
        let client = self.client.clone();
        let config = self.config.clone();
        let flusher = SqsFlusher::new(client.clone(), config.queue_url.clone());
        let ack_buffer = AckBuffer::spawn(flusher, config.ack_buffer.clone());
        let tx_ack = ack_buffer.sender();

        let (tx, rx) = mpsc::channel(config.max_in_flight * 2);
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

        let handle = tokio::spawn(async move {
            loop {
                if *shutdown_rx.borrow() {
                    break;
                }
                let resp = client
                    .receive_message()
                    .queue_url(&config.queue_url)
                    .max_number_of_messages(config.max_in_flight as i32)
                    .wait_time_seconds(config.wait_time.as_secs() as i32)
                    .visibility_timeout(config.visibility_timeout.as_secs() as i32)
                    .send()
                    .await;
                let messages = match resp {
                    Ok(o) => o.messages.unwrap_or_default(),
                    Err(e) => {
                        tracing::warn!("sqs receive error: {e}");
                        tokio::select! {
                            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => continue,
                            _ = shutdown_rx.changed() => return,
                        }
                    }
                };
                for m in messages {
                    let body = match m.body.as_deref() {
                        Some(b) => b,
                        None => continue,
                    };
                    let receipt = match m.receipt_handle.clone() {
                        Some(r) => r,
                        None => continue,
                    };
                    let serialized: SerializedEvent =
                        match serde_json::from_str::<serde_json::Value>(body) {
                            Ok(v) => match deserialize_event_value(v) {
                                Ok(s) => s,
                                Err(e) => {
                                    tracing::warn!("malformed sqs body: {e}");
                                    continue;
                                }
                            },
                            Err(e) => {
                                tracing::warn!("invalid sqs body json: {e}");
                                continue;
                            }
                        };
                    if serialized.organization != config.organization.as_str() {
                        tracing::warn!(
                            "sqs org mismatch: got `{}` expected `{}`; acking and skipping",
                            serialized.organization,
                            config.organization
                        );
                        let skip_acker = BatchedAcker::new(receipt, tx_ack.clone());
                        let _ = skip_acker.ack().await;
                        continue;
                    }
                    let event = match serialized.to_event() {
                        Ok(e) => e,
                        Err(e) => {
                            tracing::warn!("sqs to_event failed: {e}");
                            continue;
                        }
                    };
                    let acker = BatchedAcker::new(receipt, tx_ack.clone());
                    if tx.send(Ok(Message::new(event, acker))).await.is_err() {
                        return;
                    }
                }
            }
        });

        Ok(SqsStream {
            rx,
            shutdown_tx,
            handle: Some(handle),
            ack_buffer,
        })
    }
}

fn deserialize_event_value(v: serde_json::Value) -> Result<SerializedEvent> {
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing id".into()))?
        .to_string();
    let organization = v
        .get("organization")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing organization".into()))?
        .to_string();
    let namespace = v
        .get("namespace")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing namespace".into()))?
        .to_string();
    let topic = v
        .get("topic")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing topic".into()))?
        .to_string();
    let key = v
        .get("key")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing key".into()))?
        .to_string();
    let payload = v
        .get("payload")
        .cloned()
        .ok_or_else(|| Error::Serialization("missing payload".into()))?;
    let content_type = v
        .get("content_type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("missing content_type".into()))?
        .to_string();
    let metadata = v
        .get("metadata")
        .and_then(|x| serde_json::from_value(x.clone()).ok())
        .unwrap_or_default();
    let timestamp = v
        .get("timestamp")
        .and_then(|x| x.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok_or_else(|| Error::Serialization("missing/invalid timestamp".into()))?;
    let version = v.get("version").and_then(|x| x.as_u64()).unwrap_or(1);
    Ok(SerializedEvent {
        id,
        organization,
        namespace,
        topic,
        key,
        payload,
        content_type,
        metadata,
        timestamp,
        version,
    })
}
