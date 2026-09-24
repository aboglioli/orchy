use std::sync::Mutex;

use async_trait::async_trait;
use chrono::Utc;
use orchy_core::{DomainEvent, EventLog, EventQuery, RecordedEvent, Result};

#[derive(Default)]
pub struct MemoryEventLog(Mutex<Vec<RecordedEvent>>);

impl MemoryEventLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.0.lock().expect("log mutex").len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn topics(&self) -> Vec<String> {
        self.0
            .lock()
            .expect("log mutex")
            .iter()
            .map(|e| e.topic.clone())
            .collect()
    }
}

#[async_trait]
impl EventLog for MemoryEventLog {
    async fn append(&self, events: &[Box<dyn DomainEvent>]) -> Result<()> {
        let mut log = self.0.lock().expect("log mutex");
        for event in events {
            let payload = event.payload()?;
            log.push(RecordedEvent {
                topic: event.topic().as_str().to_owned(),
                key: event.key().to_string(),
                namespace: event.namespace().to_string(),
                actor: None,
                machine: None,
                payload: serde_json::from_slice(payload.data()).unwrap_or(serde_json::Value::Null),
                recorded_at: Utc::now(),
            });
        }
        Ok(())
    }

    async fn replay(&self, query: &EventQuery) -> Result<Vec<RecordedEvent>> {
        let log = self.0.lock().expect("log mutex");
        let mut found: Vec<RecordedEvent> =
            log.iter().filter(|e| query.matches(e)).cloned().collect();
        if let Some(limit) = query.limit {
            found.truncate(limit);
        }
        Ok(found)
    }
}
