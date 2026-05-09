use aws_sdk_sqs::Client;
use aws_sdk_sqs::types::{
    ChangeMessageVisibilityBatchRequestEntry, DeleteMessageBatchRequestEntry,
};

use orchy_events::io::acker::BatchFlusher;
use orchy_events::{Error, Result};

pub struct SqsFlusher {
    client: Client,
    queue_url: String,
}

impl SqsFlusher {
    pub fn new(client: Client, queue_url: impl Into<String>) -> Self {
        Self {
            client,
            queue_url: queue_url.into(),
        }
    }
}

impl BatchFlusher for SqsFlusher {
    type Token = String;

    async fn flush(&self, acks: Vec<String>) -> Result<()> {
        if acks.is_empty() {
            return Ok(());
        }
        let entries: Vec<DeleteMessageBatchRequestEntry> = acks
            .into_iter()
            .enumerate()
            .map(|(i, h)| {
                DeleteMessageBatchRequestEntry::builder()
                    .id(i.to_string())
                    .receipt_handle(h)
                    .build()
                    .map_err(|e| Error::Store(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        self.client
            .delete_message_batch()
            .queue_url(&self.queue_url)
            .set_entries(Some(entries))
            .send()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn flush_nack(&self, nacks: Vec<String>) -> Result<()> {
        if nacks.is_empty() {
            return Ok(());
        }
        let entries: Vec<ChangeMessageVisibilityBatchRequestEntry> = nacks
            .into_iter()
            .enumerate()
            .map(|(i, h)| {
                ChangeMessageVisibilityBatchRequestEntry::builder()
                    .id(i.to_string())
                    .receipt_handle(h)
                    .visibility_timeout(0)
                    .build()
                    .map_err(|e| Error::Store(e.to_string()))
            })
            .collect::<Result<Vec<_>>>()?;
        self.client
            .change_message_visibility_batch()
            .queue_url(&self.queue_url)
            .set_entries(Some(entries))
            .send()
            .await
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }
}
