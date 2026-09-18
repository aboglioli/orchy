use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{EventLog, Id, Page, PageRequest, Result, Task, TaskQuery, TaskStore};

use crate::eventlog::MemoryEventLog;

pub struct MemoryTaskStore {
    tasks: Mutex<BTreeMap<Id, Task>>,
    log: Arc<MemoryEventLog>,
}

impl MemoryTaskStore {
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

    async fn find(&self, query: &TaskQuery, page: PageRequest) -> Result<Page<Task>> {
        let mut matched: Vec<Task> = self
            .snapshot()
            .into_iter()
            .filter(|t| query.matches(t))
            .collect();
        matched.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(Page::slice(matched, page))
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

    async fn delete(&self, id: &Id) -> Result<()> {
        self.tasks.lock().expect("task mutex").remove(id);
        Ok(())
    }
}
