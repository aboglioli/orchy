use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;

use orchy_application::ReaderFactory;
use orchy_core::error::Result;
use orchy_events::io::{BoxAcker, BoxStream, Reader, ReaderExt};
use orchy_events::{Namespace, OrganizationId, StartFrom, Topic};

use crate::reader::{PgReader, PgReaderConfig};

pub struct PgReaderFactory {
    pool: PgPool,
}

impl PgReaderFactory {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ReaderFactory for PgReaderFactory {
    async fn build_history_reader(
        &self,
        organization: OrganizationId,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        limit: usize,
        topics: Option<Vec<Topic>>,
        namespace_prefix: Option<Namespace>,
    ) -> Result<Arc<dyn Reader<Acker = BoxAcker, Stream = BoxStream> + Send + Sync>> {
        let reader = PgReader::new(
            self.pool.clone(),
            PgReaderConfig {
                organization,
                consumer_group_id: None,
                start_from: StartFrom::Timestamp(since),
                topics,
                namespace_prefix,
                end_at: Some(until),
                limit: Some(limit),
                batch_size: limit.clamp(1, 100),
                poll_interval: Duration::from_millis(100),
            },
        );
        Ok(Arc::new(reader.into_boxed()))
    }
}
