use std::collections::HashSet;
use std::sync::Arc;

use crate::domain::TaskAggregate;
use crate::entities::{CreateTask, Task, TaskFilter};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::{AgentId, Namespace, TaskId, TaskStatus};

pub struct TaskService<S: Store> {
    store: Arc<S>,
}

impl<S: Store> TaskService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn create(&self, cmd: CreateTask) -> Result<Task> {
        for dep_id in &cmd.depends_on {
            if self.store.get_task(dep_id).await?.is_none() {
                return Err(Error::NotFound(format!("dependency task {dep_id}")));
            }
        }
        self.store.create_task(cmd).await
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
        let mut task = self.get(id).await?;

        if !self.all_deps_completed(&task.depends_on).await? {
            return Err(Error::DependencyNotMet(id.to_string()));
        }

        TaskAggregate::claim(&mut task, *agent)?;
        self.store.update_task(&task).await
    }

    pub async fn get_next(
        &self,
        agent: &AgentId,
        roles: &[String],
        namespace: Option<Namespace>,
    ) -> Result<Option<Task>> {
        let mut candidates: Vec<Task> = Vec::new();

        for role in roles {
            let filter = TaskFilter {
                namespace: namespace.clone(),
                status: Some(TaskStatus::Pending),
                assigned_role: Some(role.clone()),
                ..Default::default()
            };
            let mut tasks = self.store.list_tasks(filter).await?;
            tasks.sort_by(|a, b| b.priority.cmp(&a.priority));
            candidates.extend(tasks);
        }

        let mut seen = HashSet::new();
        candidates.retain(|t| seen.insert(t.id));
        candidates.sort_by(|a, b| b.priority.cmp(&a.priority));

        for mut task in candidates {
            if self.all_deps_completed(&task.depends_on).await? {
                match TaskAggregate::claim(&mut task, *agent) {
                    Ok(()) => {
                        let claimed = self.store.update_task(&task).await?;
                        return Ok(Some(claimed));
                    }
                    Err(Error::InvalidTransition { .. }) => continue,
                    Err(e) => return Err(e),
                }
            }
        }

        Ok(None)
    }

    pub async fn start(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let mut task = self.get(id).await?;
        TaskAggregate::start(&mut task, agent)?;
        self.store.update_task(&task).await
    }

    pub async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        TaskAggregate::complete(&mut task, summary)?;
        let task = self.store.update_task(&task).await?;
        self.unblock_dependents(&task.id).await?;
        Ok(task)
    }

    pub async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        TaskAggregate::fail(&mut task, reason)?;
        self.store.update_task(&task).await
    }

    pub async fn reassign(&self, id: &TaskId, new_agent: &AgentId) -> Result<Task> {
        self.store
            .get_agent(new_agent)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {new_agent}")))?;

        let mut task = self.get(id).await?;
        TaskAggregate::reassign(&mut task, *new_agent)?;
        self.store.update_task(&task).await
    }

    pub async fn release(&self, id: &TaskId) -> Result<Task> {
        let mut task = self.get(id).await?;
        TaskAggregate::release(&mut task)?;
        self.store.update_task(&task).await
    }

    pub async fn release_agent_tasks(&self, agent: &AgentId) -> Result<Vec<TaskId>> {
        let filter = TaskFilter {
            claimed_by: Some(*agent),
            ..Default::default()
        };
        let tasks = self.store.list_tasks(filter).await?;
        let mut released = Vec::with_capacity(tasks.len());
        for task in &tasks {
            self.release(&task.id).await?;
            released.push(task.id);
        }
        Ok(released)
    }

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
