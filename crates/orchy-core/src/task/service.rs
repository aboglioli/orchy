use std::collections::HashSet;
use std::sync::Arc;

use super::{
    Priority, ReviewId, ReviewRequest, ReviewStore, SubtaskDef, Task, TaskFilter, TaskId,
    TaskStatus, TaskStore, TaskWatcher, TaskWithContext, WatcherStore,
};
use crate::agent::{AgentId, AgentStore};
use crate::error::{Error, Result};
use crate::message::{Message, MessageStore, MessageTarget};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;

pub struct TaskService<TS: TaskStore, S: AgentStore + WatcherStore + MessageStore + ReviewStore> {
    task_store: Arc<TS>,
    store: Arc<S>,
}

impl<TS: TaskStore, S: AgentStore + WatcherStore + MessageStore + ReviewStore> TaskService<TS, S> {
    pub fn new(task_store: Arc<TS>, store: Arc<S>) -> Self {
        Self { task_store, store }
    }

    pub async fn create(&self, mut task: Task) -> Result<()> {
        for dep_id in task.depends_on() {
            if self.task_store.find_by_id(dep_id).await?.is_none() {
                return Err(Error::NotFound(format!("dependency task {dep_id}")));
            }
        }
        self.task_store.save(&mut task).await
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
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn sorted_pending_for_roles(
        &self,
        roles: &[String],
        namespace: Option<Namespace>,
    ) -> Result<Vec<Task>> {
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
        Ok(candidates)
    }

    pub async fn peek_next(
        &self,
        roles: &[String],
        namespace: Option<Namespace>,
    ) -> Result<Option<Task>> {
        let candidates = self.sorted_pending_for_roles(roles, namespace).await?;
        for task in candidates {
            if self.all_deps_completed(task.depends_on()).await? {
                return Ok(Some(task));
            }
        }
        Ok(None)
    }

    pub async fn get_next(
        &self,
        agent: &AgentId,
        roles: &[String],
        namespace: Option<Namespace>,
    ) -> Result<Option<Task>> {
        let candidates = self.sorted_pending_for_roles(roles, namespace).await?;

        for mut task in candidates {
            if self.all_deps_completed(task.depends_on()).await? {
                match task.claim(*agent) {
                    Ok(()) => {
                        self.task_store.save(&mut task).await?;
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
        self.task_store.save(&mut task).await?;
        self.notify_watchers(&task, "started").await;
        Ok(task)
    }

    pub async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.complete(summary)?;
        self.task_store.save(&mut task).await?;
        self.notify_watchers(&task, "completed").await;
        self.resolve_dependents(task.id()).await?;
        Ok(task)
    }

    pub async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.fail(reason)?;
        self.task_store.save(&mut task).await?;
        self.notify_watchers(&task, "failed").await;
        self.notify_blocked_dependents_terminated(&task, "failed")
            .await;
        Ok(task)
    }

    pub async fn cancel(&self, id: &TaskId, reason: Option<String>) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.cancel(reason)?;
        self.task_store.save(&mut task).await?;
        self.notify_watchers(&task, "cancelled").await;
        self.notify_blocked_dependents_terminated(&task, "cancelled")
            .await;
        Ok(task)
    }

    pub async fn update_details(
        &self,
        id: &TaskId,
        title: Option<String>,
        description: Option<String>,
        priority: Option<Priority>,
    ) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.update_details(title, description, priority)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn unblock_manual(&self, id: &TaskId) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.unblock()?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn add_note(
        &self,
        id: &TaskId,
        author: Option<AgentId>,
        body: String,
    ) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.add_note(author, body)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn tag(&self, id: &TaskId, tag: String) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.add_tag(tag)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn untag(&self, id: &TaskId, tag: &str) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.remove_tag(tag)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn move_task(&self, id: &TaskId, namespace: Namespace) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.move_to(namespace)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn assign(&self, id: &TaskId, new_agent: &AgentId) -> Result<Task> {
        AgentStore::find_by_id(&*self.store, new_agent)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {new_agent}")))?;

        let mut task = self.get(id).await?;
        task.assign(*new_agent)?;
        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn release(&self, id: &TaskId) -> Result<Task> {
        let mut task = self.get(id).await?;
        task.release()?;
        self.task_store.save(&mut task).await?;
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
            let mut task = Task::new(
                parent.org_id().clone(),
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
            self.task_store.save(&mut task).await?;
            children.push(task);
        }

        for child in &children {
            parent.add_dependency(child.id())?;
        }
        parent.block()?;
        self.task_store.save(&mut parent).await?;

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
        self.task_store.save(&mut original).await?;

        let mut new_tasks = Vec::with_capacity(replacements.len());
        for def in replacements {
            let mut task = Task::new(
                original.org_id().clone(),
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
            self.task_store.save(&mut task).await?;
            new_tasks.push(task);
        }

        Ok((original, new_tasks))
    }

    pub async fn merge_tasks(
        &self,
        task_ids: &[TaskId],
        title: String,
        description: String,
        created_by: Option<AgentId>,
    ) -> Result<(Task, Vec<Task>)> {
        if task_ids.len() < 2 {
            return Err(Error::InvalidInput(
                "merge requires at least 2 tasks".into(),
            ));
        }

        let mut sources = Vec::with_capacity(task_ids.len());
        for id in task_ids {
            sources.push(self.get(id).await?);
        }

        let org_id = sources[0].org_id().clone();
        let project = sources[0].project().clone();
        for task in &sources {
            if *task.project() != project {
                return Err(Error::InvalidInput(format!(
                    "task {} belongs to project {}, expected {}",
                    task.id(),
                    task.project(),
                    project
                )));
            }
            if !task.status().is_mergeable() {
                return Err(Error::InvalidInput(format!(
                    "task {} has status {} which cannot be merged",
                    task.id(),
                    task.status()
                )));
            }
        }

        let source_ids: HashSet<TaskId> = task_ids.iter().copied().collect();

        let priority = sources
            .iter()
            .map(|t| t.priority())
            .max()
            .unwrap_or(Priority::default());

        let mut roles_set = HashSet::new();
        for task in &sources {
            for role in task.assigned_roles() {
                roles_set.insert(role.clone());
            }
        }
        let assigned_roles: Vec<String> = roles_set.into_iter().collect();

        let mut deps_set = HashSet::new();
        for task in &sources {
            for dep in task.depends_on() {
                if !source_ids.contains(dep) {
                    deps_set.insert(*dep);
                }
            }
        }
        let depends_on: Vec<TaskId> = deps_set.into_iter().collect();

        let parent_id = {
            let first_parent = sources[0].parent_id();
            if sources.iter().all(|t| t.parent_id() == first_parent) {
                first_parent
            } else {
                None
            }
        };

        let namespace = sources[0].namespace().clone();
        let is_blocked = !depends_on.is_empty() && !self.all_deps_completed(&depends_on).await?;

        let mut merged = Task::new(
            org_id,
            project,
            namespace,
            parent_id,
            title,
            description,
            priority,
            assigned_roles,
            depends_on,
            created_by,
            is_blocked,
        )?;

        for task in &sources {
            for note in task.notes() {
                merged.add_note(note.author(), note.body().to_string())?;
            }
        }

        self.task_store.save(&mut merged).await?;

        let mut cancelled = Vec::with_capacity(sources.len());
        for mut task in sources {
            task.cancel(Some(format!("merged into {}", merged.id())))?;
            self.task_store.save(&mut task).await?;
            cancelled.push(task);
        }

        for source_id in &source_ids {
            let children = self
                .task_store
                .list(TaskFilter {
                    parent_id: Some(*source_id),
                    ..Default::default()
                })
                .await?;

            for mut child in children {
                child.set_parent_id(Some(merged.id()));
                self.task_store.save(&mut child).await?;
            }
        }

        for status in [
            TaskStatus::Pending,
            TaskStatus::Blocked,
            TaskStatus::Claimed,
        ] {
            let tasks = self
                .task_store
                .list(TaskFilter {
                    project: Some(merged.project().clone()),
                    status: Some(status),
                    ..Default::default()
                })
                .await?;

            for mut task in tasks {
                if source_ids.contains(&task.id()) || task.id() == merged.id() {
                    continue;
                }

                let mut changed = false;
                for source_id in &source_ids {
                    if task.depends_on().contains(source_id) {
                        task.replace_dependency(source_id, merged.id());
                        changed = true;
                    }
                }

                if changed {
                    self.task_store.save(&mut task).await?;
                }
            }
        }

        Ok((merged, cancelled))
    }

    pub async fn add_dependency(&self, task_id: &TaskId, dependency_id: &TaskId) -> Result<Task> {
        self.get(dependency_id).await?;

        let mut task = self.get(task_id).await?;

        if matches!(
            task.status(),
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot add dependency to task {} with status {}",
                task_id,
                task.status()
            )));
        }

        task.add_dependency(*dependency_id)?;

        if !self.all_deps_completed(task.depends_on()).await?
            && task.status() == TaskStatus::Pending
        {
            task.block()?;
        }

        self.task_store.save(&mut task).await?;
        Ok(task)
    }

    pub async fn remove_dependency(
        &self,
        task_id: &TaskId,
        dependency_id: &TaskId,
    ) -> Result<Task> {
        let mut task = self.get(task_id).await?;
        task.remove_dependency(dependency_id)?;

        if task.status() == TaskStatus::Blocked
            && self.all_deps_completed(task.depends_on()).await?
        {
            task.unblock()?;
        }

        self.task_store.save(&mut task).await?;
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

    fn resolve_dependents(
        &self,
        completed_id: TaskId,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move { self.resolve_dependents_inner(completed_id).await })
    }

    async fn resolve_dependents_inner(&self, completed_id: TaskId) -> Result<()> {
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
                    task.auto_complete(summary)?;
                    self.task_store.save(&mut task).await?;
                    self.resolve_dependents(task_id).await?;
                } else {
                    task.unblock()?;
                    self.task_store.save(&mut task).await?;
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

    pub async fn watch(
        &self,
        task_id: &TaskId,
        agent_id: AgentId,
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
    ) -> Result<TaskWatcher> {
        self.get(task_id).await?;
        let mut watcher = TaskWatcher::new(*task_id, agent_id, org_id, project, namespace);
        WatcherStore::save(&*self.store, &mut watcher).await?;
        Ok(watcher)
    }

    pub async fn unwatch(&self, task_id: &TaskId, agent_id: &AgentId) -> Result<()> {
        WatcherStore::delete(&*self.store, task_id, agent_id).await
    }

    pub async fn request_review(
        &self,
        task_id: &TaskId,
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        requester: AgentId,
        reviewer: Option<AgentId>,
        reviewer_role: Option<String>,
    ) -> Result<ReviewRequest> {
        self.get(task_id).await?;
        let mut review = ReviewRequest::new(
            *task_id,
            org_id.clone(),
            project.clone(),
            namespace.clone(),
            requester,
            reviewer,
            reviewer_role.clone(),
        );
        ReviewStore::save(&*self.store, &mut review).await?;

        let body = format!(
            "Review requested for task {} (review {})",
            task_id,
            review.id()
        );
        let target = if let Some(agent) = reviewer {
            MessageTarget::Agent(agent)
        } else if let Some(role) = reviewer_role {
            MessageTarget::Role(role)
        } else {
            MessageTarget::Broadcast
        };
        let mut msg = Message::new(org_id, project, namespace, requester, target, body, None);
        let _ = MessageStore::save(&*self.store, &mut msg).await;

        Ok(review)
    }

    pub async fn resolve_review(
        &self,
        review_id: &ReviewId,
        resolver: AgentId,
        approved: bool,
        comments: Option<String>,
    ) -> Result<ReviewRequest> {
        let mut review = ReviewStore::find_by_id(&*self.store, review_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("review {review_id}")))?;

        if approved {
            review.approve(comments)?;
        } else {
            review.reject(comments)?;
        }
        ReviewStore::save(&*self.store, &mut review).await?;

        let body = format!(
            "Review {} for task {}: {}",
            review.id(),
            review.task_id(),
            review.status()
        );
        let mut msg = Message::new(
            review.org_id().clone(),
            review.project().clone(),
            review.namespace().clone(),
            resolver,
            MessageTarget::Agent(review.requester()),
            body,
            None,
        );
        let _ = MessageStore::save(&*self.store, &mut msg).await;

        Ok(review)
    }

    pub async fn get_review(&self, id: &ReviewId) -> Result<ReviewRequest> {
        ReviewStore::find_by_id(&*self.store, id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("review {id}")))
    }

    pub async fn list_reviews_for_task(&self, task_id: &TaskId) -> Result<Vec<ReviewRequest>> {
        ReviewStore::find_by_task(&*self.store, task_id).await
    }

    pub async fn pending_reviews_for_agent(&self, agent_id: &AgentId) -> Result<Vec<ReviewRequest>> {
        ReviewStore::find_pending_for_agent(&*self.store, agent_id).await
    }

    async fn notify_watchers(&self, task: &Task, event: &str) {
        let watchers = WatcherStore::find_watchers(&*self.store, &task.id()).await;
        if let Ok(watchers) = watchers {
            for watcher in watchers {
                let body = format!("[watch] task {} ({}): {}", task.id(), task.title(), event);
                let mut msg = Message::new(
                    watcher.org_id().clone(),
                    watcher.project().clone(),
                    watcher.namespace().clone(),
                    watcher.agent_id(),
                    MessageTarget::Agent(watcher.agent_id()),
                    body,
                    None,
                );
                let _ = MessageStore::save(&*self.store, &mut msg).await;
            }
        }
    }

    async fn notify_blocked_dependents_terminated(&self, failed_task: &Task, event: &str) {
        let blocked = self
            .task_store
            .list(TaskFilter {
                project: Some(failed_task.project().clone()),
                status: Some(TaskStatus::Blocked),
                ..Default::default()
            })
            .await;

        if let Ok(tasks) = blocked {
            for task in tasks {
                if task.depends_on().contains(&failed_task.id()) {
                    if let Some(agent) = task.assigned_to() {
                        let body = format!(
                            "[dependency-{}] task {} ({}) depends on {} task {} ({})",
                            event,
                            task.id(),
                            task.title(),
                            event,
                            failed_task.id(),
                            failed_task.title(),
                        );
                        let mut msg = Message::new(
                            task.org_id().clone(),
                            task.project().clone(),
                            task.namespace().clone(),
                            agent,
                            MessageTarget::Agent(agent),
                            body,
                            None,
                        );
                        let _ = MessageStore::save(&*self.store, &mut msg).await;
                    }
                    self.notify_watchers(&task, &format!("dependency {}", event))
                        .await;
                }
            }
        }
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
            let summary = child.result_summary().unwrap_or("(no summary)");
            parts.push(format!(
                "- [{}] {}: {}",
                child.status(),
                child.title(),
                summary
            ));
        }
        Ok(format!(
            "All {} subtasks completed:\n{}",
            children.len(),
            parts.join("\n")
        ))
    }
}
