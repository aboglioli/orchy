use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_events::SerializedEvent;

pub use orchy_events::EventQuery;

pub struct PollUpdatesCommand {
    pub org_id: String,
    pub since: String,
    pub limit: Option<u32>,
}

pub struct PollUpdates {
    events: Arc<dyn EventQuery>,
}

impl PollUpdates {
    pub fn new(events: Arc<dyn EventQuery>) -> Self {
        Self { events }
    }

    pub async fn execute(&self, cmd: PollUpdatesCommand) -> Result<Vec<SerializedEvent>> {
        let since = cmd
            .since
            .parse::<chrono::DateTime<chrono::Utc>>()
            .map_err(|e| Error::InvalidInput(format!("invalid timestamp: {e}")))?;

        let limit = cmd.limit.unwrap_or(50) as usize;

        self.events
            .query_events(&cmd.org_id, since, limit)
            .await
            .map_err(Error::Store)
    }
}
