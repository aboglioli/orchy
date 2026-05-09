use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures::{Stream, StreamExt};
use rdkafka::ClientConfig;
use rdkafka::Message as KafkaMessage;
use rdkafka::TopicPartitionList;
use rdkafka::consumer::{Consumer, StreamConsumer};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use orchy_events::io::acker::{AckBuffer, BatchedAcker};
use orchy_events::io::filters::{NamespacePrefixFilter, TopicFilter};
use orchy_events::io::{Acker, Filter, Message, Reader};
use orchy_events::{Error, Event, Result, SerializedEvent, StartFrom};

use crate::flusher::{KafkaFlusher, KafkaOffsetToken};
use crate::reader_config::{KafkaReaderConfig, PartitionAssignment};

pub struct KafkaReader {
    consumer: Arc<StreamConsumer>,
    config: KafkaReaderConfig,
}

impl KafkaReader {
    pub fn new(config: KafkaReaderConfig) -> Result<Self> {
        let mut cfg = ClientConfig::new();
        cfg.set("bootstrap.servers", config.brokers.join(","))
            .set("group.id", config.consumer_group_id.as_str())
            .set("enable.auto.commit", "false")
            .set(
                "session.timeout.ms",
                config.session_timeout.as_millis().to_string(),
            )
            .set(
                "auto.offset.reset",
                match config.start_from {
                    StartFrom::Earliest => "earliest",
                    StartFrom::Latest | StartFrom::Timestamp(_) => "latest",
                },
            );
        let consumer: StreamConsumer = cfg.create().map_err(|e| Error::Store(e.to_string()))?;

        match &config.partition_assignment {
            PartitionAssignment::All => {
                let topic_refs: Vec<&str> =
                    config.kafka_topics.iter().map(|t| t.as_str()).collect();
                consumer
                    .subscribe(&topic_refs)
                    .map_err(|e| Error::Store(e.to_string()))?;
            }
            PartitionAssignment::Specific(parts) => {
                let offset = match config.start_from {
                    StartFrom::Earliest => rdkafka::Offset::Beginning,
                    StartFrom::Latest => rdkafka::Offset::End,
                    StartFrom::Timestamp(_) => rdkafka::Offset::Stored,
                };
                let mut tpl = TopicPartitionList::new();
                for topic in &config.kafka_topics {
                    for p in parts {
                        tpl.add_partition_offset(topic, *p, offset)
                            .map_err(|e| Error::Store(e.to_string()))?;
                    }
                }
                consumer
                    .assign(&tpl)
                    .map_err(|e| Error::Store(e.to_string()))?;
            }
        }

        if let StartFrom::Timestamp(ts) = config.start_from {
            apply_timestamp_seek(&consumer, &config, ts)?;
        }

        Ok(Self {
            consumer: Arc::new(consumer),
            config,
        })
    }
}

fn apply_timestamp_seek(
    consumer: &StreamConsumer,
    config: &KafkaReaderConfig,
    ts: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let mut tpl = TopicPartitionList::new();
    for topic in &config.kafka_topics {
        let metadata = consumer
            .fetch_metadata(Some(topic), std::time::Duration::from_secs(10))
            .map_err(|e| Error::Store(e.to_string()))?;
        let topic_meta = metadata
            .topics()
            .iter()
            .find(|t| t.name() == topic)
            .ok_or_else(|| Error::Store(format!("topic {topic} not found")))?;
        for p in topic_meta.partitions() {
            tpl.add_partition_offset(
                topic,
                p.id(),
                rdkafka::Offset::Offset(ts.timestamp_millis()),
            )
            .map_err(|e| Error::Store(e.to_string()))?;
        }
    }
    let resolved = consumer
        .offsets_for_times(tpl, std::time::Duration::from_secs(10))
        .map_err(|e| Error::Store(e.to_string()))?;
    consumer
        .assign(&resolved)
        .map_err(|e| Error::Store(e.to_string()))?;
    Ok(())
}

pub struct KafkaStream {
    rx: mpsc::Receiver<Result<Message<BatchedAcker<KafkaOffsetToken>>>>,
    cancel: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
    ack_buffer: Arc<AckBuffer<KafkaFlusher>>,
}

impl Drop for KafkaStream {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(h) = self.handle.take() {
            h.abort();
        }
        let buf = Arc::clone(&self.ack_buffer);
        tokio::spawn(async move {
            let _ = buf.shutdown().await;
        });
    }
}

impl Stream for KafkaStream {
    type Item = Result<Message<BatchedAcker<KafkaOffsetToken>>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl Reader for KafkaReader {
    type Acker = BatchedAcker<KafkaOffsetToken>;
    type Stream = KafkaStream;

    async fn read(&self) -> Result<Self::Stream> {
        let consumer = Arc::clone(&self.consumer);
        let config = self.config.clone();
        let flusher = KafkaFlusher::new(Arc::clone(&consumer));
        let ack_buffer = AckBuffer::spawn(flusher, config.ack_buffer.clone());
        let tx_ack = ack_buffer.sender();

        // Sized to one Kafka poll batch so the producer can stage exactly
        // one batch ahead of the consumer; blocks producer otherwise.
        let (tx, rx) = mpsc::channel(config.max_poll_records);
        let cancel = CancellationToken::new();
        let cancel_for_task = cancel.clone();

        let handle = tokio::spawn(async move {
            let topic_filter = config.orchy_topics.clone().map(TopicFilter::new);
            let ns_filter = config
                .namespace_prefix
                .clone()
                .map(NamespacePrefixFilter::new);
            let mut delivered = 0usize;
            let mut stream = consumer.stream();

            loop {
                if cancel_for_task.is_cancelled() {
                    break;
                }
                let next = tokio::select! {
                    n = stream.next() => n,
                    _ = cancel_for_task.cancelled() => return,
                };
                let msg = match next {
                    None => return,
                    Some(Err(e)) => {
                        tracing::warn!("kafka stream error: {e}");
                        continue;
                    }
                    Some(Ok(m)) => m,
                };
                let body = match msg.payload() {
                    Some(p) => p,
                    None => continue,
                };
                let serialized: SerializedEvent =
                    match serde_json::from_slice::<serde_json::Value>(body) {
                        Ok(v) => match deserialize_event_value(v) {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::warn!("kafka body parse error: {e}");
                                continue;
                            }
                        },
                        Err(e) => {
                            tracing::warn!("kafka invalid json: {e}");
                            continue;
                        }
                    };
                if serialized.organization != config.organization.as_str() {
                    continue;
                }
                if let Some(ts) = config.end_at
                    && serialized.timestamp > ts
                {
                    break;
                }
                let event: Event = match serialized.to_event() {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("kafka to_event error: {e}");
                        continue;
                    }
                };
                let token = KafkaOffsetToken {
                    topic: msg.topic().to_owned(),
                    partition: msg.partition(),
                    offset: msg.offset(),
                };
                if let Some(f) = topic_filter.as_ref()
                    && !f.matches(&event)
                {
                    let skip_acker = BatchedAcker::new(token, tx_ack.clone());
                    let _ = skip_acker.ack().await;
                    continue;
                }
                if let Some(f) = ns_filter.as_ref()
                    && !f.matches(&event)
                {
                    let skip_acker = BatchedAcker::new(token, tx_ack.clone());
                    let _ = skip_acker.ack().await;
                    continue;
                }
                let acker = BatchedAcker::new(token, tx_ack.clone());
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
        });

        Ok(KafkaStream {
            rx,
            cancel,
            handle: Some(handle),
            ack_buffer,
        })
    }
}

fn deserialize_event_value(v: serde_json::Value) -> Result<SerializedEvent> {
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("id".into()))?
        .to_owned();
    let organization = v
        .get("organization")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("organization".into()))?
        .to_owned();
    let namespace = v
        .get("namespace")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("namespace".into()))?
        .to_owned();
    let topic = v
        .get("topic")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("topic".into()))?
        .to_owned();
    let key = v
        .get("key")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("key".into()))?
        .to_owned();
    let payload = v
        .get("payload")
        .cloned()
        .ok_or_else(|| Error::Serialization("payload".into()))?;
    let content_type = v
        .get("content_type")
        .and_then(|x| x.as_str())
        .ok_or_else(|| Error::Serialization("content_type".into()))?
        .to_owned();
    let metadata = v
        .get("metadata")
        .and_then(|x| serde_json::from_value(x.clone()).ok())
        .unwrap_or_default();
    let timestamp = v
        .get("timestamp")
        .and_then(|x| x.as_str())
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok_or_else(|| Error::Serialization("timestamp".into()))?;
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
