mod collector;
mod error;
mod event;
mod metadata;
mod namespace;
mod organization;
mod payload;
mod serialization;
mod topic;

pub use collector::EventCollector;
pub use error::{Error, Result};
pub use event::{Event, EventId, RestoreEvent};
pub use metadata::Metadata;
pub use namespace::EventNamespace;
pub use organization::Organization;
pub use payload::{ContentType, Payload};
pub use serialization::SerializedEvent;
pub use topic::Topic;

use std::future::Future;

pub trait EventLog: Send + Sync {
    fn append(&self, events: &[SerializedEvent]) -> impl Future<Output = Result<()>> + Send;
    fn list(
        &self,
        filter: EventFilter,
    ) -> impl Future<Output = Result<Vec<SerializedEvent>>> + Send;
}

#[derive(Debug, Clone, Default)]
pub struct EventFilter {
    pub organization: Option<String>,
    pub namespace: Option<String>,
    pub topic: Option<String>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: Option<usize>,
}
