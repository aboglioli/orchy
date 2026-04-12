use chrono::Utc;

use orchy_core::agent::AgentId;
use orchy_core::error::{Error, Result};
use orchy_core::task::{CreateTask, Task, TaskFilter, TaskId, TaskStatus, TaskStore};

use crate::MemoryBackend;

impl TaskStore for MemoryBackend {
    async fn create(&self, cmd: CreateTask) -> Result<Task> {
        let now = Utc::now();
        let id = TaskId::new();

        let initial_status = if cmd.depends_on.is_empty() {
            TaskStatus::Pending
        } else {
            let tasks = self.tasks.read().map_err(|e| Error::Store(e.to_string()))?;
            let all_completed = cmd.depends_on.iter().all(|dep_id| {
                tasks
                    .get(dep_id)
                    .map(|t| t.status == TaskStatus::Completed)
                    .unwrap_or(false)
            });
            if all_completed {
                TaskStatus::Pending
            } else {
                TaskStatus::Blocked
            }
        };

        let task = Task {
            id,
            namespace: cmd.namespace,
            title: cmd.title,
            description: cmd.description,
            status: initial_status,
            priority: cmd.priority,
            assigned_roles: cmd.assigned_roles,
            claimed_by: None,
            claimed_at: None,
            depends_on: cmd.depends_on,
            result_summary: None,
            created_by: cmd.created_by,
            created_at: now,
            updated_at: now,
        };

        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        tasks.insert(task.id, task.clone());
        Ok(task)
    }

    async fn get(&self, id: &TaskId) -> Result<Option<Task>> {
        let tasks = self.tasks.read().map_err(|e| Error::Store(e.to_string()))?;
        Ok(tasks.get(id).cloned())
    }

    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        let tasks = self.tasks.read().map_err(|e| Error::Store(e.to_string()))?;

        let mut results: Vec<Task> = tasks
            .values()
            .filter(|t| {
                if let Some(ref ns) = filter.namespace {
                    if !t.namespace.starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if t.namespace.project() != project.as_ref() {
                        return false;
                    }
                }
                if let Some(ref status) = filter.status {
                    if t.status != *status {
                        return false;
                    }
                }
                if let Some(ref role) = filter.assigned_role {
                    if !t.assigned_roles.is_empty() && !t.assigned_roles.contains(role) {
                        return false;
                    }
                }
                if let Some(ref claimed) = filter.claimed_by {
                    if t.claimed_by.as_ref() != Some(claimed) {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        results.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(results)
    }

    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        task.status.transition_to(TaskStatus::Claimed)?;
        task.status = TaskStatus::Claimed;
        task.claimed_by = Some(*agent);
        task.claimed_at = Some(Utc::now());
        task.updated_at = Utc::now();

        Ok(task.clone())
    }

    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        task.status.transition_to(TaskStatus::Completed)?;
        task.status = TaskStatus::Completed;
        task.result_summary = summary;
        task.updated_at = Utc::now();

        Ok(task.clone())
    }

    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        task.status.transition_to(TaskStatus::Failed)?;
        task.status = TaskStatus::Failed;
        task.result_summary = reason;
        task.updated_at = Utc::now();

        Ok(task.clone())
    }

    async fn release(&self, id: &TaskId) -> Result<Task> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        task.status.transition_to(TaskStatus::Pending)?;
        task.status = TaskStatus::Pending;
        task.claimed_by = None;
        task.claimed_at = None;
        task.updated_at = Utc::now();

        Ok(task.clone())
    }

    async fn update(&self, task: &Task) -> Result<Task> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let existing = tasks
            .get_mut(&task.id)
            .ok_or_else(|| Error::NotFound(format!("task {}", task.id)))?;

        *existing = task.clone();
        existing.updated_at = Utc::now();
        Ok(existing.clone())
    }

    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let task = tasks
            .get_mut(id)
            .ok_or_else(|| Error::NotFound(format!("task {id}")))?;

        task.status = status;
        task.updated_at = Utc::now();
        Ok(())
    }
}
