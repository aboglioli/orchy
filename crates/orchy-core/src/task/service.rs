use std::collections::HashSet;
use std::sync::Arc;

use super::{Task, TaskFilter, TaskId, TaskStatus, TaskStore};
use crate::agent::{AgentId, AgentStore};
use crate::error::{Error, Result};
use crate::namespace::Namespace;

pub struct TaskService<TS: TaskStore, AS: AgentStore> {
    task_store: Arc<TS>,
    agent_store: Arc<AS>,
}

impl<TS: TaskStore, AS: AgentStore> TaskService<TS, AS> {
    pub fn new(task_store: Arc<TS>, agent_store: Arc<AS>) -> Self {
        Self {
            task_store,
            agent_store,
        }
    }

    pub async fn create(&self, task: Task) -> Result<()> {
        for dep_id in task.depends_on() {
            if self.task_store.find_by_id(dep_id).await?.is_none() {
                return Err(Error::NotFound(format!("dependency task {dep_id}")));
            }
        }
        self.task_store.save(&task).await
    }

    pub async fn get(&self, id: &TaskId) -> Result<Task> {
        self.task_store
            .find_by_id(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>> {
        self.task_store.list(filter).await
    }

    pub async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task> {
        let mut task = self.get(id).await?;

        if !self.all_deps_completed(task.depends_on()).await? {
            return Err(Error::DependencyNotMet(id.to_string()));
        }

        task.claim(*agent)?;
        self.task_store.save(&task).await?;
        Ok(task)
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
            let mut tasks = self.task_store.list(filter).await?;
            tasks.sort_by_key(|t| std::cmp::Reverse(t.priority()));
            candidates.extend(tasks);
        }

        let mut seen = HashSet::new();
        candidates.retain(|t| seen.insert(t.id()));
        candidates.sort_by_key(|t| std::cmp::Reverse(t.priority()));

        for mut task in candidates {
            if self.all_deps_completed(task.depends_on()).await? {
                match task.claim(*agent) {
                    Ok(()) => {
                        self.task_store.save(&task).await?;
                        return Ok(Some(task));
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
        task.start(agent)?;
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.complete(summary)?;
        self.task_store.save(&task).await?;
        self.unblock_dependents(task.id()).await?;
        Ok(task)
    }

    pub async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.fail(reason)?;
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn add_note(
        &self,
        id: &TaskId,
        author: Option<AgentId>,
        body: String,
    ) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.add_note(author, body);
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn move_task(&self, id: &TaskId, namespace: Namespace) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.move_to(namespace);
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn assign(&self, id: &TaskId, new_agent: &AgentId) -> Result<Task> {
        self.agent_store
            .find_by_id(new_agent)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {new_agent}")))?;

        let mut task = self.get(id).await?;
        task.assign(*new_agent)?;
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn release(&self, id: &TaskId) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.release()?;
        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn release_agent_tasks(&self, agent: &AgentId) -> Result<Vec<TaskId>> {
        let filter = TaskFilter {
            claimed_by: Some(*agent),
            ..Default::default()
        };
        let tasks = self.task_store.list(filter).await?;
        let mut released = Vec::with_capacity(tasks.len());
        for task in &tasks {
            self.release(&task.id()).await?;
            released.push(task.id());
        }
        Ok(released)
    }

    async fn all_deps_completed(&self, deps: &[TaskId]) -> Result<bool> {
        for dep_id in deps {
            let dep = self
                .task_store
                .find_by_id(dep_id)
                .await?
                .ok_or_else(|| Error::NotFound(format!("dependency task {dep_id}")))?;
            if dep.status() != TaskStatus::Completed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn unblock_dependents(&self, completed_id: TaskId) -> Result<()> {
        let blocked = self
            .task_store
            .list(TaskFilter {
                status: Some(TaskStatus::Blocked),
                ..Default::default()
            })
            .await?;

        for mut task in blocked {
            if task.depends_on().contains(&completed_id)
                && self.all_deps_completed(task.depends_on()).await?
            {
                task.unblock();
                self.task_store.save(&task).await?;
            }
        }

        Ok(())
    }
}
