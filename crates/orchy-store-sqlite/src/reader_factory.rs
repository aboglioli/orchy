use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use orchy_application::ReaderFactory;
use orchy_core::error::Result;
use orchy_events::io::{BoxAcker, BoxStream, Reader, ReaderExt};
use orchy_events::{Namespace, OrganizationId, StartFrom, Topic};

use crate::SqliteConn;
use crate::reader::{SqliteReader, SqliteReaderConfig};

pub struct SqliteReaderFactory {
    conn: SqliteConn,
}

impl SqliteReaderFactory {
    pub fn new(conn: SqliteConn) -> Self {
        Self { conn }
    }
}

#[async_trait]
impl ReaderFactory for SqliteReaderFactory {
    async fn build_history_reader(
        &self,
        organization: OrganizationId,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        limit: usize,
        topics: Option<Vec<Topic>>,
        namespace_prefix: Option<Namespace>,
    ) -> Result<Arc<dyn Reader<Acker = BoxAcker, Stream = BoxStream> + Send + Sync>> {
        let reader = SqliteReader::new(
            self.conn.clone(),
            SqliteReaderConfig {
                organization,
                consumer_group_id: None,
                start_from: StartFrom::Timestamp(since),
                topics,
                namespace_prefix,
                end_at: Some(until),
                limit: Some(limit),
                poll_interval: Duration::from_millis(50),
            },
        );
        Ok(Arc::new(reader.into_boxed()))
    }
}
