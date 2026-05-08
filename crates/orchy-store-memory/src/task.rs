use std::sync::Arc;

use async_trait::async_trait;

use orchy_core::error::{Error, Result};
use orchy_core::pagination::{Page, PageParams};
use orchy_core::task::{Task, TaskFilter, TaskId, TaskStore};

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
        if let Some(pv) = task.persisted_version() {
            let stored = self.state.tasks.get(&task.id()).ok_or_else(|| {
                Error::not_found(orchy_core::error::Resource::Task, task.id().to_string())
            })?;
            if stored.version() != pv {
                return Err(Error::version_mismatch(pv, stored.version()));
            }
        }
        task.mark_persisted();
        self.state.tasks.insert(task.id(), task.clone());

        let events = task.drain_events();
        self.state.append_events(events).await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>> {
        Ok(self.state.tasks.get(id).map(|r| r.clone()))
    }

    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>> {
        let mut results: Vec<Task> = self
            .state
            .tasks
            .iter()
            .filter(|entry| {
                let t = entry.value();
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
            .map(|e| e.value().clone())
            .collect();

        results.sort_by_key(|t| std::cmp::Reverse(t.priority()));

        Ok(crate::apply_cursor_pagination(results, &page, |t| {
            t.id().to_string()
        }))
    }

    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>> {
        Ok(ids
            .iter()
            .filter_map(|id| self.state.tasks.get(id).map(|r| r.clone()))
            .collect())
    }
}
