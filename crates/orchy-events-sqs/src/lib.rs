//! SQS-backed event reader and writer for Orchy.
//!
//! # Supported semantics
//!
//! - `StartFrom`: only [`orchy_events::StartFrom::Latest`] is supported (SQS
//!   has no replay).
//! - Ack: deletes the message from the queue.
//! - Nack: resets visibility timeout to zero for immediate redelivery.
//! - Ordering: per-message group within a single FIFO queue; not guaranteed
//!   across messages on standard queues.

mod flusher;
mod reader;
mod reader_config;
mod writer;

pub use flusher::SqsFlusher;
pub use reader::SqsReader;
pub use reader_config::SqsReaderConfig;
pub use writer::SqsWriter;
