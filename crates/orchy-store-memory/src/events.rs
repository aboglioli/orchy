use std::sync::Arc;

use async_trait::async_trait;

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

impl Writer for MemoryEventWriter {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let serialized = SerializedEvent::from_event(event)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        {
            let mut store = self.state.events.write().await;
            store.push(serialized);
        }
        self.state.events_notify.notify_waiters();
        Ok(())
    }

    async fn write_all(&self, events: &[Event]) -> orchy_events::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let mut serialized = Vec::with_capacity(events.len());
        for event in events {
            serialized.push(
                SerializedEvent::from_event(event)
                    .map_err(|e| orchy_events::Error::Store(e.to_string()))?,
            );
        }
        {
            let mut store = self.state.events.write().await;
            store.extend(serialized);
        }
        self.state.events_notify.notify_waiters();
        Ok(())
    }
}
