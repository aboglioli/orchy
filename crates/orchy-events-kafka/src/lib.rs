//! Kafka-backed event reader and writer for Orchy.
//!
//! # Supported semantics
//!
//! - `StartFrom`: [`orchy_events::StartFrom::Earliest`],
//!   [`orchy_events::StartFrom::Latest`], and timestamp-based starts are
//!   supported via consumer-group seek.
//! - Ack: commits the offset to the consumer group.
//! - Nack: no-op; redelivery depends on consumer rebalance, restart, or seek.
//! - Ordering: preserved within a partition; not guaranteed across partitions.

mod flusher;
mod reader;
mod reader_config;
mod writer;

pub use flusher::{KafkaFlusher, KafkaOffsetToken};
pub use reader::KafkaReader;
pub use reader_config::{KafkaReaderConfig, PartitionAssignment};
pub use writer::KafkaWriter;
