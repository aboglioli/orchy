use std::sync::Arc;

use chrono::{DateTime, Utc};
use orchy_core::{EventLog, EventQuery, Id};
use serde::{Deserialize, Serialize};

use crate::dto::EventDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadEventsCommand {
    pub topic_prefix: Option<String>,
    pub key: Option<String>,
    pub actor: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub limit: Option<usize>,
}

pub struct ReadEvents {
    log: Arc<dyn EventLog>,
}

impl ReadEvents {
    pub fn new(log: Arc<dyn EventLog>) -> Self {
        Self { log }
    }

    pub async fn execute(&self, cmd: ReadEventsCommand) -> ApplicationResult<Vec<EventDto>> {
        let query = EventQuery {
            topic_prefix: cmd.topic_prefix,
            key: cmd.key.as_deref().map(Id::new).transpose()?,
            actor: cmd.actor,
            since: cmd.since,
            limit: cmd.limit,
        };
        let events = self.log.replay(&query).await?;
        Ok(events.iter().map(EventDto::from).collect())
    }
}
