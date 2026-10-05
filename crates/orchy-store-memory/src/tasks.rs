use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{EventLog, Id, Result, Task, TaskQuery, TaskStore};

use crate::eventlog::MemoryEventLog;

pub struct MemoryTaskStore {
    tasks: Mutex<BTreeMap<Id, Task>>,
    log: Arc<MemoryEventLog>,
}

impl MemoryTaskStore {
    pub(crate) fn state(&self) -> BTreeMap<Id, Task> {
        self.tasks.lock().expect("tasks lock").clone()
    }

    pub(crate) fn restore_state(&self, state: BTreeMap<Id, Task>) {
        *self.tasks.lock().expect("tasks lock") = state;
    }

    pub fn new(log: Arc<MemoryEventLog>) -> Self {
        Self {
            tasks: Mutex::new(BTreeMap::new()),
            log,
        }
    }

    fn snapshot(&self) -> Vec<Task> {
        self.tasks
            .lock()
            .expect("task mutex")
            .values()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl TaskStore for MemoryTaskStore {
    async fn get(&self, id: &Id) -> Result<Option<Task>> {
        Ok(self.tasks.lock().expect("task mutex").get(id).cloned())
    }

    async fn matching(&self, query: &TaskQuery) -> Result<Vec<Task>> {
        let mut matched: Vec<Task> = self
            .snapshot()
            .into_iter()
            .filter(|t| query.matches(t))
            .collect();
        matched.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(matched)
    }

    async fn children_of(&self, parent: &Id) -> Result<Vec<Task>> {
        let mut children: Vec<Task> = self
            .snapshot()
            .into_iter()
            .filter(|t| t.parent() == Some(parent))
            .collect();
        children.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(children)
    }

    async fn save(&self, task: &mut Task) -> Result<()> {
        let events = task.drain_events();
        self.tasks
            .lock()
            .expect("task mutex")
            .insert(task.id().clone(), task.clone());
        self.log.append(&events).await
    }
}
