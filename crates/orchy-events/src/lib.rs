//! Core event model and async IO traits for Orchy.
//!
//! The crate provides static-dispatch async traits by default and dyn-safe
//! bridge traits for dependency injection and runtime composition.
//!
//! # Core concepts
//!
//! - [`Event`] for domain and integration events
//! - [`Payload`] for JSON, text, and binary data
//! - [`io::Reader`] and [`io::Writer`] for event IO
//! - [`io::Handler`] and [`BackgroundConsumer`] for event processing
//! - [`Snapshot`] for event-sourced aggregate snapshots
//! - [`PartitionKey`] for deterministic event routing
//! - [`ConsumerStream`] for scoping consumer offsets

mod collector;
mod consumer_group_id;
mod cursor;
mod error;
mod event;
mod event_key;
pub mod io;
mod metadata;
mod namespace;
mod organization;
mod partition;
mod payload;
mod selector;
mod serialization;
mod snapshot;
mod start_from;
mod stream;
mod topic;

pub use collector::EventCollector;
pub use consumer_group_id::ConsumerGroupId;
pub use cursor::{EventCursor, EventSequence};
pub use error::{Error, Result};
pub use event::{Event, EventId, RestoreEvent};
pub use event_key::EventKey;
pub use metadata::Metadata;
pub use namespace::Namespace;
pub use organization::OrganizationId;
pub use partition::PartitionKey;
pub use payload::{ContentType, Payload};
pub use selector::EventSelector;
pub use serialization::SerializedEvent;
pub use snapshot::{Snapshot, SnapshotEventId};
pub use start_from::StartFrom;
pub use stream::ConsumerStream;
pub use topic::Topic;

pub use io::{
    Acker, AckerExt, Filter, FilterExt, FilteredHandler, Handler, HandlerExt, Message, Reader,
    ReaderExt, Writer, WriterExt,
};
pub use io::{
    ArcAcker, ArcFilter, ArcHandler, ArcReader, ArcWriter, BoxAcker, BoxFilter, BoxHandler,
    BoxReader, BoxStream, BoxWriter,
};
pub use io::{BackgroundConsumer, ConsumerHandle};
