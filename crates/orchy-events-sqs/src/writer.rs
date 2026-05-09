use async_trait::async_trait;

use aws_sdk_sqs::Client;
use aws_sdk_sqs::types::SendMessageBatchRequestEntry;

use orchy_events::io::Writer;
use orchy_events::{Error, Event, Result, SerializedEvent};

const SQS_BATCH_MAX: usize = 10;
const SQS_PAYLOAD_MAX: usize = 256 * 1024;

pub struct SqsWriter {
    client: Client,
    queue_url: String,
}

impl SqsWriter {
    pub fn new(client: Client, queue_url: impl Into<String>) -> Self {
        Self {
            client,
            queue_url: queue_url.into(),
        }
    }

    fn serialize_body(event: &Event) -> Result<String> {
        let s = SerializedEvent::from_event(event)?;
        let body = serde_json::to_string(&serde_json::json!({
            "id": s.id,
            "organization": s.organization,
            "namespace": s.namespace,
            "topic": s.topic,
            "key": s.key,
            "payload": s.payload,
            "content_type": s.content_type,
            "metadata": s.metadata,
            "timestamp": s.timestamp,
            "version": s.version,
        }))
        .map_err(|e| Error::Serialization(e.to_string()))?;
        if body.len() > SQS_PAYLOAD_MAX {
            return Err(Error::InvalidPayload(format!(
                "event body {} bytes exceeds 256 KB",
                body.len()
            )));
        }
        Ok(body)
    }

    async fn send_batch(&self, entries: Vec<SendMessageBatchRequestEntry>) -> Result<()> {
        self.client
            .send_message_batch()
            .queue_url(&self.queue_url)
            .set_entries(Some(entries))
            .send()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}

impl Writer for SqsWriter {
    async fn write(&self, event: &Event) -> Result<()> {
        let body = Self::serialize_body(event)?;
        self.client
            .send_message()
            .queue_url(&self.queue_url)
            .message_body(body)
            .send()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn write_all(&self, events: &[Event]) -> Result<()> {
        let mut current: Vec<SendMessageBatchRequestEntry> = Vec::new();
        let mut current_bytes = 0usize;

        for (id_counter, event) in events.iter().enumerate() {
            let body = Self::serialize_body(event)?;
            let body_len = body.len();
            let would_overflow =
                current.len() == SQS_BATCH_MAX || (current_bytes + body_len) > SQS_PAYLOAD_MAX;
            if would_overflow {
                let drained = std::mem::take(&mut current);
                self.send_batch(drained).await?;
                current_bytes = 0;
            }
            let entry = SendMessageBatchRequestEntry::builder()
                .id(id_counter.to_string())
                .message_body(body)
                .build()
                .map_err(|e| Error::Store(e.to_string()))?;
            current.push(entry);
            current_bytes += body_len;
        }
        if !current.is_empty() {
            self.send_batch(current).await?;
        }
        Ok(())
    }
}
