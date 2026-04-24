use orchy_application::EventQuery;
use orchy_core::error::Result;
use orchy_events::SerializedEvent;

pub(crate) struct MemoryEventQueryAdapter(pub orchy_store_memory::MemoryEventQuery);

#[async_trait::async_trait]
impl EventQuery for MemoryEventQueryAdapter {
    async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        self.0.query_events(organization, since, limit).await
    }
}

pub(crate) struct SqliteEventQueryAdapter(pub orchy_store_sqlite::SqliteEventQuery);

#[async_trait::async_trait]
impl EventQuery for SqliteEventQueryAdapter {
    async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        self.0.query_events(organization, since, limit)
    }
}

pub(crate) struct PgEventQueryAdapter(pub orchy_store_pg::PgEventQuery);

#[async_trait::async_trait]
impl EventQuery for PgEventQueryAdapter {
    async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        self.0.query_events(organization, since, limit).await
    }
}
