use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_events::io::Writer;
use orchy_events::{Event, SerializedEvent};

use crate::MemoryBackend;

#[async_trait]
impl Writer for MemoryBackend {
    async fn write(&self, event: &Event) -> orchy_events::Result<()> {
        let serialized = SerializedEvent::from_event(event)
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        let mut store = self
            .events
            .write()
            .map_err(|e| orchy_events::Error::Store(e.to_string()))?;
        store.push(serialized);
        Ok(())
    }
}

impl MemoryBackend {
    pub fn list_events(&self) -> Result<Vec<SerializedEvent>> {
        let store = self
            .events
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(store.clone())
    }
}
