use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::Result;
use orchy_events::io::Writer;
use orchy_events::{Event, SerializedEvent};

use crate::MemoryState;

pub struct MemoryEventWriter {
    state: Arc<MemoryState>,
}

impl MemoryEventWriter {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl Writer for MemoryEventWriter {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let serialized = SerializedEvent::from_event(event)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        let mut store = self.state.events.write().await;
        store.push(serialized);
        Ok(())
    }
}

pub struct MemoryEventQuery {
    state: Arc<MemoryState>,
}

impl MemoryEventQuery {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }

    pub async fn list_events(&self) -> Result<Vec<SerializedEvent>> {
        let store = self.state.events.read().await;
        Ok(store.clone())
    }

    pub async fn query_events(
        &self,
        organization: &str,
        since: chrono::DateTime<chrono::Utc>,
        limit: usize,
    ) -> Result<Vec<SerializedEvent>> {
        let store = self.state.events.read().await;
        let mut filtered: Vec<_> = store
            .iter()
            .filter(|e| e.organization == organization && e.timestamp >= since)
            .cloned()
            .collect();
        filtered.sort_by_key(|b| std::cmp::Reverse(b.timestamp));
        filtered.truncate(limit);
        filtered.reverse();
        Ok(filtered)
    }
}
