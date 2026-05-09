use std::result::Result as StdResult;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Result};
use orchy_events::io::{BoxReader, Reader};
use orchy_events::{Event, Namespace, OrganizationId, Topic};

pub struct PollUpdatesCommand {
    pub organization: String,
    pub since: String,
    pub limit: Option<u32>,
    pub topics: Option<Vec<String>>,
    pub namespace_prefix: Option<String>,
}

#[async_trait]
pub trait ReaderFactory: Send + Sync {
    async fn build_history_reader(
        &self,
        organization: OrganizationId,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        limit: usize,
        topics: Option<Vec<Topic>>,
        namespace_prefix: Option<Namespace>,
    ) -> Result<BoxReader>;
}

pub struct PollUpdates {
    factory: Arc<dyn ReaderFactory>,
}

impl PollUpdates {
    pub fn new(factory: Arc<dyn ReaderFactory>) -> Self {
        Self { factory }
    }

    pub async fn execute(&self, cmd: PollUpdatesCommand) -> ApplicationResult<Vec<Event>> {
        let organization = OrganizationId::new(&cmd.organization)?;
        let since: DateTime<Utc> = cmd.since.parse().map_err(|e: chrono::ParseError| {
            Error::invalid_input(format!("invalid timestamp: {e}"))
        })?;
        let topics = match cmd.topics {
            None => None,
            Some(ts) => Some(
                ts.into_iter()
                    .map(Topic::new)
                    .collect::<StdResult<Vec<_>, _>>()?,
            ),
        };
        let namespace_prefix = match cmd.namespace_prefix {
            None => None,
            Some(s) => Some(Namespace::new(s)?),
        };
        let limit = cmd.limit.unwrap_or(50) as usize;
        let until = Utc::now();

        let reader = self
            .factory
            .build_history_reader(organization, since, until, limit, topics, namespace_prefix)
            .await?;

        let mut stream = reader.read().await?;
        let mut events = Vec::with_capacity(limit);
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            let _ = msg.ack().await;
            events.push(msg.into_event());
            if events.len() >= limit {
                break;
            }
        }
        Ok(events)
    }
}
