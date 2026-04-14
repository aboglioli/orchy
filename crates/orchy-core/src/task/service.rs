use std::collections::HashSet;
use std::sync::Arc;

use super::{SubtaskDef, Task, TaskFilter, TaskId, TaskStatus, TaskStore, TaskWithContext};
use crate::agent::{AgentId, AgentStore};
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};

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
        self.resolve_dependents(task.id()).await?;
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
            assigned_to: Some(*agent),
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

    pub async fn split_task(
        &self,
        parent_id: &TaskId,
        subtasks: Vec<SubtaskDef>,
        created_by: Option<AgentId>,
    ) -> Result<(Task, Vec<Task>)> {
        let mut parent = self.get(parent_id).await?;

        if matches!(
            parent.status(),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot split task {} with status {}",
                parent_id,
                parent.status()
            )));
        }

        let mut children = Vec::with_capacity(subtasks.len());

        for def in subtasks {
            let task = Task::new(
                parent.project().clone(),
                parent.namespace().clone(),
                Some(*parent_id),
                def.title,
                def.description,
                def.priority,
                def.assigned_roles,
                def.depends_on,
                created_by,
                false,
            )?;
            self.task_store.save(&task).await?;
            children.push(task);
        }

        for child in &children {
            parent.add_dependency(child.id());
        }
        parent.block()?;
        self.task_store.save(&parent).await?;

        Ok((parent, children))
    }

    pub async fn replace_task(
        &self,
        task_id: &TaskId,
        reason: Option<String>,
        replacements: Vec<SubtaskDef>,
        created_by: Option<AgentId>,
    ) -> Result<(Task, Vec<Task>)> {
        let mut original = self.get(task_id).await?;
        let cancel_reason = reason.unwrap_or_else(|| "replaced by new tasks".to_string());
        original.cancel(Some(cancel_reason))?;
        self.task_store.save(&original).await?;

        let mut new_tasks = Vec::with_capacity(replacements.len());
        for def in replacements {
            let task = Task::new(
                original.project().clone(),
                original.namespace().clone(),
                original.parent_id(),
                def.title,
                def.description,
                def.priority,
                def.assigned_roles,
                def.depends_on,
                created_by,
                false,
            )?;
            self.task_store.save(&task).await?;
            new_tasks.push(task);
        }

        Ok((original, new_tasks))
    }

    pub async fn add_dependency(
        &self,
        task_id: &TaskId,
        dependency_id: &TaskId,
    ) -> Result<Task> {

        self.get(dependency_id).await?;

        let mut task = self.get(task_id).await?;

        if matches!(task.status(), TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled) {
            return Err(Error::InvalidInput(format!(
                "cannot add dependency to task {} with status {}",
                task_id, task.status()
            )));
        }

        task.add_dependency(*dependency_id);


        if !self.all_deps_completed(task.depends_on()).await? {
            if task.status() == TaskStatus::Pending {
                task.block()?;
            }
        }

        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn remove_dependency(
        &self,
        task_id: &TaskId,
        dependency_id: &TaskId,
    ) -> Result<Task> {
        let mut task = self.get(task_id).await?;
        task.remove_dependency(dependency_id);


        if task.status() == TaskStatus::Blocked
            && self.all_deps_completed(task.depends_on()).await?
        {
            task.unblock();
        }

        self.task_store.save(&task).await?;
        Ok(task)
    }

    pub async fn suggest_roles(
        &self,
        project: &ProjectId,
        namespace: Option<Namespace>,
    ) -> Result<Vec<String>> {
        let mut role_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();


        for status in &[TaskStatus::Pending, TaskStatus::Blocked] {
            let filter = TaskFilter {
                project: Some(project.clone()),
                namespace: namespace.clone(),
                status: Some(*status),
                ..Default::default()
            };
            let tasks = self.task_store.list(filter).await?;
            for task in &tasks {
                for role in task.assigned_roles() {
                    *role_counts.entry(role.clone()).or_insert(0) += 1;
                }
            }
        }

        let mut roles: Vec<(String, usize)> = role_counts.into_iter().collect();
        roles.sort_by(|a, b| b.1.cmp(&a.1));

        Ok(roles.into_iter().take(3).map(|(r, _)| r).collect())
    }

    pub async fn get_with_context(&self, id: &TaskId) -> Result<TaskWithContext> {
        let task = self.get(id).await?;

        let mut ancestors = Vec::new();
        let mut current_parent_id = task.parent_id();
        while let Some(pid) = current_parent_id {
            match self.task_store.find_by_id(&pid).await? {
                Some(parent) => {
                    current_parent_id = parent.parent_id();
                    ancestors.push(parent);
                }
                None => break,
            }
        }

        let children = self
            .task_store
            .list(TaskFilter {
                parent_id: Some(*id),
                ..Default::default()
            })
            .await?;

        Ok(TaskWithContext {
            task,
            ancestors,
            children,
        })
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

    async fn resolve_dependents(&self, completed_id: TaskId) -> Result<()> {
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
                if self.has_children(&task).await? {
                    let summary = self.children_summaries(&task).await?;
                    let task_id = task.id();
                    task.auto_complete(summary);
                    self.task_store.save(&task).await?;
                    self.resolve_dependents(task_id).await?;
                } else {
                    task.unblock();
                    self.task_store.save(&task).await?;
                }
            }
        }

        Ok(())
    }

    async fn has_children(&self, task: &Task) -> Result<bool> {
        let children = self
            .task_store
            .list(TaskFilter {
                parent_id: Some(task.id()),
                ..Default::default()
            })
            .await?;
        Ok(!children.is_empty())
    }

    async fn children_summaries(&self, task: &Task) -> Result<String> {
        let children = self
            .task_store
            .list(TaskFilter {
                parent_id: Some(task.id()),
                ..Default::default()
            })
            .await?;

        let mut parts = Vec::new();
        for child in &children {
            let summary = child
                .result_summary()
                .unwrap_or("(no summary)");
            parts.push(format!("- [{}] {}: {}", child.status(), child.title(), summary));
        }
        Ok(format!(
            "All {} subtasks completed:\n{}",
            children.len(),
            parts.join("\n")
        ))
    }
}
