use std::time::Duration;

use chrono::{DateTime, Utc};

use orchy_events::io::acker::AckBufferConfig;
use orchy_events::{ConsumerGroupId, Namespace, OrganizationId, StartFrom, Topic};

#[derive(Clone)]
pub enum PartitionAssignment {
    All,
    Specific(Vec<i32>),
}

#[derive(Clone)]
pub struct KafkaReaderConfig {
    pub brokers: Vec<String>,
    pub kafka_topics: Vec<String>,
    pub consumer_group_id: ConsumerGroupId,
    pub organization: OrganizationId,
    pub start_from: StartFrom,
    pub orchy_topics: Option<Vec<Topic>>,
    pub namespace_prefix: Option<Namespace>,
    pub session_timeout: Duration,
    pub max_poll_records: usize,
    pub partition_assignment: PartitionAssignment,
    pub end_at: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
    pub ack_buffer: AckBufferConfig,
}

impl KafkaReaderConfig {
    pub fn streaming(
        brokers: Vec<String>,
        kafka_topics: Vec<String>,
        consumer_group_id: ConsumerGroupId,
        organization: OrganizationId,
    ) -> Self {
        Self {
            brokers,
            kafka_topics,
            consumer_group_id,
            organization,
            start_from: StartFrom::Latest,
            orchy_topics: None,
            namespace_prefix: None,
            session_timeout: Duration::from_secs(30),
            max_poll_records: 100,
            partition_assignment: PartitionAssignment::All,
            end_at: None,
            limit: None,
            ack_buffer: AckBufferConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_defaults() {
        let c = KafkaReaderConfig::streaming(
            vec!["broker:9092".to_string()],
            vec!["t".to_string()],
            ConsumerGroupId::new("g").unwrap(),
            OrganizationId::new("o").unwrap(),
        );
        assert_eq!(c.kafka_topics.len(), 1);
        assert!(matches!(c.partition_assignment, PartitionAssignment::All));
        assert!(matches!(c.start_from, StartFrom::Latest));
    }
}
