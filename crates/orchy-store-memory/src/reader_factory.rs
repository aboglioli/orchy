use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use orchy_application::ReaderFactory;
use orchy_core::error::Result;
use orchy_events::io::{BoxReader, ReaderExt};
use orchy_events::{ConsumerStream, Namespace, OrganizationId, StartFrom, Topic};

use crate::MemoryState;
use crate::reader::{MemoryReader, MemoryReaderConfig, OffsetMap};

pub struct MemoryReaderFactory {
    state: Arc<MemoryState>,
    offsets: Arc<RwLock<OffsetMap>>,
}

impl MemoryReaderFactory {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self {
            state,
            offsets: Arc::new(RwLock::new(OffsetMap::new())),
        }
    }
}

#[async_trait]
impl ReaderFactory for MemoryReaderFactory {
    async fn build_history_reader(
        &self,
        organization: OrganizationId,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        limit: usize,
        topics: Option<Vec<Topic>>,
        namespace_prefix: Option<Namespace>,
    ) -> Result<BoxReader> {
        let reader = MemoryReader::new(
            Arc::clone(&self.state),
            Arc::clone(&self.offsets),
            MemoryReaderConfig {
                organization,
                consumer_group_id: None,
                stream: ConsumerStream::default(),
                start_from: StartFrom::Timestamp(since),
                topics,
                namespace_prefix,
                end_at: Some(until),
                limit: Some(limit),
            },
        );
        Ok(reader.into_boxed())
    }
}
