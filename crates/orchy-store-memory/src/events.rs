use orchy_events::{EventFilter, EventStore, SerializedEvent};
use orchy_events::{Error, Result};

use crate::MemoryBackend;

impl EventStore for MemoryBackend {
    async fn append(&self, events: &[SerializedEvent]) -> Result<()> {
        let mut store = self
            .events
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        for event in events {
            store.push(event.clone());
        }
        Ok(())
    }

    async fn list(&self, filter: EventFilter) -> Result<Vec<SerializedEvent>> {
        let store = self
            .events
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        let mut results: Vec<SerializedEvent> = store
            .iter()
            .filter(|e| {
                if let Some(ref org) = filter.organization {
                    if e.organization != *org {
                        return false;
                    }
                }
                if let Some(ref ns) = filter.namespace {
                    if e.namespace != *ns {
                        return false;
                    }
                }
                if let Some(ref topic) = filter.topic {
                    if e.topic != *topic {
                        return false;
                    }
                }
                if let Some(ref since) = filter.since {
                    if e.timestamp < *since {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        if let Some(limit) = filter.limit {
            results.truncate(limit);
        }

        Ok(results)
    }
}
