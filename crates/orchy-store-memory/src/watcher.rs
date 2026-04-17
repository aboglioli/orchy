use async_trait::async_trait;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{TaskId, TaskWatcher, WatcherStore};

use crate::MemoryBackend;

#[async_trait]
impl WatcherStore for MemoryBackend {
    async fn save(&self, watcher: &mut TaskWatcher) -> Result<()> {
        {
            let mut watchers = self
                .watchers
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            watchers.retain(|w| {
                !(w.task_id() == watcher.task_id() && *w.agent_id() == *watcher.agent_id())
            });
            watchers.push(watcher.clone());
        }

        let events = watcher.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn delete(&self, task_id: &TaskId, agent_id: &AgentId) -> Result<()> {
        let mut watchers = self
            .watchers
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        watchers.retain(|w| !(w.task_id() == *task_id && *w.agent_id() == *agent_id));
        Ok(())
    }

    async fn find_watchers(&self, task_id: &TaskId) -> Result<Vec<TaskWatcher>> {
        let watchers = self
            .watchers
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(watchers
            .iter()
            .filter(|w| w.task_id() == *task_id)
            .cloned()
            .collect())
    }

    async fn find_by_agent(&self, agent_id: &AgentId) -> Result<Vec<TaskWatcher>> {
        let watchers = self
            .watchers
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        Ok(watchers
            .iter()
            .filter(|w| *w.agent_id() == *agent_id)
            .cloned()
            .collect())
    }
}
