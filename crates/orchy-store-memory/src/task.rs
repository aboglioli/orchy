use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};

use crate::MemoryBackend;

#[async_trait]
impl TaskStore for MemoryBackend {
    async fn save(&self, task: &mut Task) -> Result<()> {
        {
            let mut tasks = self
                .tasks
                .write()
                .map_err(|e| Error::Store(e.to_string()))?;
            tasks.insert(task.id(), task.clone());
        }

        let events = task.drain_events();
        if !events.is_empty() {
            let _ = orchy_events::io::Writer::write_all(self, &events).await;
        }

        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let tasks = self.tasks.read().map_err(|e| Error::Store(e.to_string()))?;
        Ok(tasks.get(id).cloned())
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let tasks = self.tasks.read().map_err(|e| Error::Store(e.to_string()))?;

        let mut results: Vec<Task> = tasks
            .values()
            .filter(|t| {
                if let Some(ref org_id) = filter.org_id {
                    if t.org_id() != org_id {
                        return false;
                    }
                }
                if let Some(ref ns) = filter.namespace {
                    if !t.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if t.project() != project {
                        return false;
                    }
                }
                if let Some(ref status) = filter.status {
                    if t.status() != *status {
                        return false;
                    }
                }
                if let Some(ref role) = filter.assigned_role {
                    if !t.assigned_roles().is_empty() && !t.assigned_roles().contains(role) {
                        return false;
                    }
                }
                if let Some(ref assigned) = filter.assigned_to {
                    match (t.assigned_to(), assigned) {
                        (Some(a), b) if *a != *b => return false,
                        (None, _) => return false,
                        _ => {}
                    }
                }
                if let Some(ref pid) = filter.parent_id {
                    if t.parent_id().as_ref() != Some(pid) {
                        return false;
                    }
                }
                if let Some(ref tag) = filter.tag {
                    if !t.tags().contains(tag) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by_key(|t| std::cmp::Reverse(t.priority()));
        Ok(results)
    }
}
