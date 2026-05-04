use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::pagination::{Page, PageParams};
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::MemoryState;

pub struct MemoryTaskStore {
    state: Arc<MemoryState>,
}

impl MemoryTaskStore {
    pub fn new(state: Arc<MemoryState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl TaskStore for MemoryTaskStore {
    async fn save(&self, task: &mut Task) -> Result<()> {
        {
            let mut tasks = self.state.tasks.write().await;
            tasks.insert(task.id(), task.clone());
        }

        let events = task.drain_events();
        if !events.is_empty() {
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| Error::Store(e.to_string()))?;
                self.state.events.write().await.push(serialized);
            }
        }

        Ok(())
    }

    async fn save_if_status(
        &self,
        task: &mut Task,
        expected_statuses: &[TaskStatus],
    ) -> Result<bool> {
        if expected_statuses.is_empty() {
            return Ok(false);
        }
        let mut tasks = self.state.tasks.write().await;
        let stored = tasks
            .get(&task.id())
            .ok_or_else(|| Error::NotFound(format!("task {}", task.id())))?;
        if !expected_statuses.contains(&stored.status()) {
            return Ok(false);
        }
        tasks.insert(task.id(), task.clone());
        let events = task.drain_events();
        if !events.is_empty() {
            let mut events_guard = self.state.events.write().await;
            for event in events {
                let serialized = orchy_events::SerializedEvent::from_event(&event)
                    .map_err(|e| Error::Store(e.to_string()))?;
                events_guard.push(serialized);
            }
        }
        Ok(true)
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        let tasks = self.state.tasks.read().await;
        Ok(tasks.get(id).cloned())
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        let tasks = self.state.tasks.read().await;

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
                if let Some(ref tag) = filter.tag {
                    if !t.tags().contains(tag) {
                        return false;
                    }
                }
                if !filter.include_archived.unwrap_or(false) && t.is_archived() {
                    return false;
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by_key(|t| std::cmp::Reverse(t.priority()));

        Ok(crate::apply_cursor_pagination(results, &page, |t| {
            t.id().to_string()
        }))
    }

    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        let tasks = self.state.tasks.read().await;
        Ok(ids.iter().filter_map(|id| tasks.get(id)).cloned().collect())
    }
}
