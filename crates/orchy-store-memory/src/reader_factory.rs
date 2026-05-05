use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::RwLock;

use orchy_application::ReaderFactory;
use orchy_core::error::Result;
use orchy_events::io::{BoxAcker, BoxStream, Reader, ReaderExt};
use orchy_events::{ConsumerGroupId, Namespace, OrganizationId, StartFrom, Topic};

use crate::MemoryState;
use crate::reader::{MemoryReader, MemoryReaderConfig};

pub struct MemoryReaderFactory {
    state: Arc<MemoryState>,
    offsets: Arc<RwLock<HashMap<ConsumerGroupId, usize>>>,
}

impl MemoryReaderFactory {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self {
            state,
            offsets: Arc::new(RwLock::new(HashMap::new())),
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
    ) -> Result<Arc<dyn Reader<Acker = BoxAcker, Stream = BoxStream> + Send + Sync>> {
        let reader = MemoryReader::new(
            self.state.clone(),
            self.offsets.clone(),
            MemoryReaderConfig {
                organization,
                consumer_group_id: None,
                start_from: StartFrom::Timestamp(since),
                topics,
                namespace_prefix,
                end_at: Some(until),
                limit: Some(limit),
            },
        );
        Ok(Arc::new(reader.into_boxed()))
    }
}
