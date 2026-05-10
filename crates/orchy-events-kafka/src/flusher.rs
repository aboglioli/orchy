use std::collections::HashMap;
use std::sync::Arc;

use rdkafka::TopicPartitionList;
use rdkafka::consumer::{Consumer, StreamConsumer};

use orchy_events::io::acker::BatchFlusher;
use orchy_events::{Error, Result};

#[derive(Clone, Debug)]
pub struct KafkaOffsetToken {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
}

/// Batches ack/nack tokens (Kafka offset positions) for the Kafka backend.
///
/// Drives ack/nack semantics for `BatchedAcker<KafkaOffsetToken>` returned by
/// the Kafka reader: `ack` commits the offset to the consumer group (highest
/// per partition wins); `nack` does not commit, so redelivery depends on
/// rebalance, restart, or seek behavior.
pub struct KafkaFlusher {
    consumer: Arc<StreamConsumer>,
}

impl KafkaFlusher {
    pub fn new(consumer: Arc<StreamConsumer>) -> Self {
        Self { consumer }
    }

    fn highest_per_partition(tokens: Vec<KafkaOffsetToken>) -> HashMap<(String, i32), i64> {
        let mut map: HashMap<(String, i32), i64> = HashMap::new();
        for t in tokens {
            let key = (t.topic, t.partition);
            map.entry(key)
                .and_modify(|cur| *cur = (*cur).max(t.offset))
                .or_insert(t.offset);
        }
        map
    }
}

impl BatchFlusher for KafkaFlusher {
    type Token = KafkaOffsetToken;

    async fn flush(&self, acks: Vec<KafkaOffsetToken>) -> Result<()> {
        if acks.is_empty() {
            return Ok(());
        }
        let highest = Self::highest_per_partition(acks);
        let mut tpl = TopicPartitionList::new();
        for ((topic, partition), offset) in highest {
            tpl.add_partition_offset(&topic, partition, rdkafka::Offset::Offset(offset + 1))
                .map_err(|e| Error::Store(e.to_string()))?;
        }
        self.consumer
            .commit(&tpl, rdkafka::consumer::CommitMode::Async)
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(())
    }

    async fn flush_nack(&self, _nacks: Vec<KafkaOffsetToken>) -> Result<()> {
        Ok(())
    }
}
