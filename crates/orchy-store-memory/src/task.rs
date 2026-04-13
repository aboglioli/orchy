use orchy_core::error::{Error, Result};
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};

use crate::MemoryBackend;

impl TaskStore for MemoryBackend {
    async fn save(&self, task: &Task) -> Result<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        tasks.insert(task.id(), task.clone());
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
                if let Some(ref ns) = filter.namespace {
                    if !t.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if t.namespace().project() != project.as_ref() {
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
                if let Some(ref claimed) = filter.claimed_by {
                    if t.claimed_by().as_ref() != Some(claimed) {
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
