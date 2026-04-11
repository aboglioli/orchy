use std::sync::Arc;

use crate::entities::{CreateTask, Task, TaskFilter};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::{AgentId, TaskId, TaskStatus};

pub struct TaskService {
    store: Arc<Store>,
}

impl TaskService {
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }

    pub async fn create(&self, cmd: CreateTask) -> Result<Task> {
        // Validate all declared dependencies exist.
        for dep_id in &cmd.depends_on {
            let dep = self.store.get_task(dep_id).await?;
            if dep.is_none() {
                return Err(Error::NotFound(format!("dependency task {dep_id}")));
            }
        }

        let task = self.store.create_task(cmd).await?;
        Ok(task)
    }

    pub async fn get(&self, id: &TaskId) -> Result<Task> {
        self.store
            .get_task(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        self.store.list_tasks(filter).await
    }

    pub async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let task = self.get(id).await?;

        if task.status != TaskStatus::Pending {
            return Err(Error::InvalidTransition {
                from: task.status.to_string(),
                to: TaskStatus::Claimed.to_string(),
            });
        }

        if !self.all_deps_completed(&task.depends_on).await? {
            return Err(Error::DependencyNotMet(id.to_string()));
        }

        self.store.claim_task(id, agent).await
    }

    /// Find the highest-priority pending task matching any of the agent's roles
    /// whose dependencies are all completed, then claim it atomically.
    pub async fn get_next(
        &self,
        agent: &AgentId,
        roles: &[String],
        namespace: Option<crate::value_objects::Namespace>,
    ) -> Result<Option<Task>> {
        // Collect candidates for each role (first match wins by priority).
        let mut candidates: Vec<Task> = Vec::new();

        for role in roles {
            let filter = TaskFilter {
                namespace: namespace.clone(),
                status: Some(TaskStatus::Pending),
                assigned_role: Some(role.clone()),
                claimed_by: None,
            };
            let mut tasks = self.store.list_tasks(filter).await?;
            // Sort descending by priority so highest-priority task comes first.
            tasks.sort_by(|a, b| b.priority.cmp(&a.priority));
            candidates.extend(tasks);
        }

        // Deduplicate while preserving order.
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|t| seen.insert(t.id));

        // Resort after dedup.
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

        // Find first claimable task (all deps completed).
        for task in &candidates {
            if self.all_deps_completed(&task.depends_on).await? {
                match self.store.claim_task(&task.id, agent).await {
                    Ok(claimed) => return Ok(Some(claimed)),
                    // If another agent claimed it first, continue searching.
                    Err(Error::InvalidTransition { .. }) => continue,
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(None)
    }

    pub async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let task = self.store.complete_task(id, summary).await?;
        self.unblock_dependents(id).await?;
        Ok(task)
    }

    pub async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        self.store.fail_task(id, reason).await
    }

    pub async fn release(&self, id: &TaskId) -> Result<Task> {
        self.store.release_task(id).await
    }

    /// Release all tasks currently claimed by the given agent back to Pending.
    pub async fn release_agent_tasks(&self, agent: &AgentId) -> Result<Vec<TaskId>> {
        let filter = TaskFilter {
            claimed_by: Some(*agent),
            ..Default::default()
        };
        let tasks = self.store.list_tasks(filter).await?;
        let mut released = Vec::with_capacity(tasks.len());
        for task in &tasks {
            self.store.release_task(&task.id).await?;
            released.push(task.id);
        }
        Ok(released)
    }

    // --- Private helpers ---

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
        for dep_id in deps {
            let dep = self
                .store
                .get_task(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// After completing a task, find all Blocked tasks that depend on it and
    /// transition any whose entire dependency set is now Completed to Pending.
    async fn unblock_dependents(&self, completed_id: &TaskId) -> Result<()> {
        let blocked = self
            .store
            .list_tasks(TaskFilter {
                status: Some(TaskStatus::Blocked),
                ..Default::default()
            })
            .await?;

        for task in &blocked {
            if task.depends_on.contains(completed_id)
                && self.all_deps_completed(&task.depends_on).await?
            {
                self.store
                    .update_task_status(&task.id, TaskStatus::Pending)
                    .await?;
            }
        }

        Ok(())
    }
}
