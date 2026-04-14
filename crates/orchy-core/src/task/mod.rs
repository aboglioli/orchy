pub mod events;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::note::Note;

use self::events as task_events;

pub trait TaskStore: Send + Sync {
    fn save(&self, task: &mut Task) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for TaskId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Blocked,
    Claimed,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub fn is_mergeable(&self) -> bool {
        matches!(
            self,
            TaskStatus::Pending | TaskStatus::Blocked | TaskStatus::Claimed
        )
    }

    pub fn can_transition_to(&self, target: &TaskStatus) -> bool {
        use TaskStatus::*;
        matches!(
            (self, target),
            (Pending, Claimed)
                | (Pending, Blocked)
                | (Pending, Cancelled)
                | (Blocked, Pending)
                | (Blocked, Cancelled)
                | (Claimed, InProgress)
                | (Claimed, Blocked)
                | (Claimed, Pending)
                | (Claimed, Failed)
                | (Claimed, Cancelled)
                | (InProgress, Blocked)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, Pending)
                | (InProgress, Cancelled)
        )
    }

    fn transition_to(self, target: TaskStatus) -> Result<TaskStatus> {
        if self.can_transition_to(&target) {
            Ok(target)
        } else {
            Err(Error::InvalidTransition {
                from: self.to_string(),
                to: target.to_string(),
            })
        }
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TaskStatus::Pending => "pending",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Claimed => "claimed",
            TaskStatus::InProgress => "in_progress",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        };
        write!(f, "{s}")
    }
}

impl FromStr for TaskStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "blocked" => Ok(TaskStatus::Blocked),
            "claimed" => Ok(TaskStatus::Claimed),
            "in_progress" => Ok(TaskStatus::InProgress),
            "completed" => Ok(TaskStatus::Completed),
            "failed" => Ok(TaskStatus::Failed),
            "cancelled" => Ok(TaskStatus::Cancelled),
            other => Err(format!("unknown task status: {other}")),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
    Critical,
}

impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Priority::Low => "low",
            Priority::Normal => "normal",
            Priority::High => "high",
            Priority::Critical => "critical",
        };
        write!(f, "{s}")
    }
}

impl FromStr for Priority {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "low" => Ok(Priority::Low),
            "normal" => Ok(Priority::Normal),
            "high" => Ok(Priority::High),
            "critical" => Ok(Priority::Critical),
            other => Err(format!("unknown priority: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    id: TaskId,
    project: ProjectId,
    namespace: Namespace,
    parent_id: Option<TaskId>,
    title: String,
    description: String,
    status: TaskStatus,
    priority: Priority,
    assigned_roles: Vec<String>,
    assigned_to: Option<AgentId>,
    assigned_at: Option<DateTime<Utc>>,
    depends_on: Vec<TaskId>,
    tags: Vec<String>,
    result_summary: Option<String>,
    notes: Vec<Note>,
    created_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Task {
    pub fn new(
        project: ProjectId,
        namespace: Namespace,
        parent_id: Option<TaskId>,
        title: String,
        description: String,
        priority: Priority,
        assigned_roles: Vec<String>,
        depends_on: Vec<TaskId>,
        created_by: Option<AgentId>,
        is_blocked: bool,
    ) -> Result<Self> {
        if title.trim().is_empty() {
            return Err(Error::InvalidInput("task title must not be empty".into()));
        }

        let now = Utc::now();
        let mut task = Self {
            id: TaskId::new(),
            project,
            namespace,
            parent_id,
            title,
            description,
            status: if is_blocked {
                TaskStatus::Blocked
            } else {
                TaskStatus::Pending
            },
            priority,
            assigned_roles,
            assigned_to: None,
            assigned_at: None,
            depends_on,
            tags: Vec::new(),
            result_summary: None,
            notes: Vec::new(),
            created_by,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        task.collector.collect(
            Event::create(
                task.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_CREATED,
                Payload::from_json(&task_events::TaskCreatedPayload {
                    task_id: task.id.to_string(),
                    project: task.project.to_string(),
                    namespace: task.namespace.to_string(),
                    title: task.title.clone(),
                    description: task.description.clone(),
                    priority: task.priority.to_string(),
                    assigned_roles: task.assigned_roles.clone(),
                    depends_on: task.depends_on.iter().map(|id| id.to_string()).collect(),
                    parent_id: task.parent_id.map(|id| id.to_string()),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(task)
    }

    pub fn restore(r: RestoreTask) -> Self {
        Self {
            id: r.id,
            project: r.project,
            namespace: r.namespace,
            parent_id: r.parent_id,
            title: r.title,
            description: r.description,
            status: r.status,
            priority: r.priority,
            assigned_roles: r.assigned_roles,
            assigned_to: r.assigned_to,
            assigned_at: r.assigned_at,
            depends_on: r.depends_on,
            tags: r.tags,
            result_summary: r.result_summary,
            notes: r.notes,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn claim(&mut self, agent: AgentId) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Claimed)?;
        self.assigned_to = Some(agent);
        self.assigned_at = Some(Utc::now());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_CLAIMED,
                Payload::from_json(&task_events::TaskClaimedPayload {
                    task_id: self.id.to_string(),
                    agent_id: agent.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn start(&mut self, agent: &AgentId) -> Result<()> {
        if self.assigned_to != Some(*agent) {
            return Err(Error::InvalidInput(format!(
                "task {} is not claimed by agent {}",
                self.id, agent
            )));
        }
        self.status = self.status.transition_to(TaskStatus::InProgress)?;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_STARTED,
                Payload::from_json(&task_events::TaskStartedPayload {
                    task_id: self.id.to_string(),
                    agent_id: agent.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn complete(&mut self, summary: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Completed)?;
        self.result_summary = summary.clone();
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_COMPLETED,
                Payload::from_json(&task_events::TaskCompletedPayload {
                    task_id: self.id.to_string(),
                    summary,
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn auto_complete(&mut self, summary: String) -> Result<()> {
        self.status = TaskStatus::Completed;
        self.result_summary = Some(summary.clone());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_AUTO_COMPLETED,
                Payload::from_json(&task_events::TaskCompletedPayload {
                    task_id: self.id.to_string(),
                    summary: Some(summary),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn fail(&mut self, reason: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Failed)?;
        self.result_summary = reason.clone();
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_FAILED,
                Payload::from_json(&task_events::TaskFailedPayload {
                    task_id: self.id.to_string(),
                    reason,
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn release(&mut self) -> Result<()> {
        if !matches!(self.status, TaskStatus::Claimed | TaskStatus::InProgress) {
            return Err(Error::InvalidTransition {
                from: self.status.to_string(),
                to: TaskStatus::Pending.to_string(),
            });
        }
        self.status = self.status.transition_to(TaskStatus::Pending)?;
        self.assigned_to = None;
        self.assigned_at = None;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_RELEASED,
                Payload::from_json(&task_events::TaskReleasedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn assign(&mut self, new_agent: AgentId) -> Result<()> {
        if !matches!(self.status, TaskStatus::Claimed | TaskStatus::InProgress) {
            return Err(Error::InvalidInput(format!(
                "task {} cannot be reassigned from status {}",
                self.id, self.status
            )));
        }
        self.status = TaskStatus::Claimed;
        self.assigned_to = Some(new_agent);
        self.assigned_at = Some(Utc::now());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_ASSIGNED,
                Payload::from_json(&task_events::TaskAssignedPayload {
                    task_id: self.id.to_string(),
                    agent_id: new_agent.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn block(&mut self) -> Result<()> {
        if self.status == TaskStatus::Blocked {
            return Ok(());
        }
        self.status = self.status.transition_to(TaskStatus::Blocked)?;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_BLOCKED,
                Payload::from_json(&task_events::TaskBlockedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn unblock(&mut self) -> Result<()> {
        if self.status != TaskStatus::Blocked {
            return Ok(());
        }
        self.status = TaskStatus::Pending;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_UNBLOCKED,
                Payload::from_json(&task_events::TaskUnblockedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn cancel(&mut self, reason: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Cancelled)?;
        self.result_summary = reason.clone();
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_CANCELLED,
                Payload::from_json(&task_events::TaskCancelledPayload {
                    task_id: self.id.to_string(),
                    reason,
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn add_dependency(&mut self, dep: TaskId) -> Result<()> {
        if self.depends_on.contains(&dep) {
            return Ok(());
        }
        self.depends_on.push(dep);
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_DEPENDENCY_ADDED,
                Payload::from_json(&task_events::TaskDependencyAddedPayload {
                    task_id: self.id.to_string(),
                    dependency_id: dep.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn remove_dependency(&mut self, dep: &TaskId) -> Result<()> {
        self.depends_on.retain(|d| d != dep);
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_DEPENDENCY_REMOVED,
                Payload::from_json(&task_events::TaskDependencyRemovedPayload {
                    task_id: self.id.to_string(),
                    dependency_id: dep.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }

    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn parent_id(&self) -> Option<TaskId> {
        self.parent_id
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn status(&self) -> TaskStatus {
        self.status
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
    pub fn assigned_roles(&self) -> &[String] {
        &self.assigned_roles
    }
    pub fn assigned_to(&self) -> Option<AgentId> {
        self.assigned_to
    }
    pub fn assigned_at(&self) -> Option<DateTime<Utc>> {
        self.assigned_at
    }
    pub fn depends_on(&self) -> &[TaskId] {
        &self.depends_on
    }
    pub fn tags(&self) -> &[String] {
        &self.tags
    }
    pub fn add_tag(&mut self, tag: String) -> Result<()> {
        if self.tags.contains(&tag) {
            return Ok(());
        }
        self.tags.push(tag.clone());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_TAGGED,
                Payload::from_json(&task_events::TaskTaggedPayload {
                    task_id: self.id.to_string(),
                    tag,
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }
    pub fn remove_tag(&mut self, tag: &str) -> Result<()> {
        let Some(pos) = self.tags.iter().position(|t| t == tag) else {
            return Ok(());
        };
        self.tags.remove(pos);
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_TAG_REMOVED,
                Payload::from_json(&task_events::TaskTagRemovedPayload {
                    task_id: self.id.to_string(),
                    tag: tag.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }
    pub fn result_summary(&self) -> Option<&str> {
        self.result_summary.as_deref()
    }
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }
    pub fn add_note(&mut self, author: Option<AgentId>, body: String) -> Result<()> {
        self.notes.push(Note::new(author, body.clone()));
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_NOTE_ADDED,
                Payload::from_json(&task_events::TaskNoteAddedPayload {
                    task_id: self.id.to_string(),
                    body,
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }
    pub fn set_parent_id(&mut self, parent_id: Option<TaskId>) {
        self.parent_id = parent_id;
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_PARENT_CHANGED,
            Payload::from_json(&task_events::TaskParentChangedPayload {
                task_id: self.id.to_string(),
                parent_id: self.parent_id.map(|id| id.to_string()),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn replace_dependency(&mut self, old: &TaskId, new: TaskId) {
        for dep in &mut self.depends_on {
            if dep == old {
                *dep = new;
            }
        }
        self.depends_on.dedup();
        self.updated_at = Utc::now();

        let _ = Event::create(
            self.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_DEPENDENCY_REPLACED,
            Payload::from_json(&task_events::TaskDependencyReplacedPayload {
                task_id: self.id.to_string(),
                old_dependency_id: old.to_string(),
                new_dependency_id: new.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));
    }

    pub fn move_to(&mut self, namespace: Namespace) -> Result<()> {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.project.as_ref(),
                task_events::NAMESPACE,
                task_events::TOPIC_MOVED,
                Payload::from_json(&task_events::TaskMovedPayload {
                    task_id: self.id.to_string(),
                    from_namespace,
                    to_namespace: self.namespace.to_string(),
                })
                .map_err(|e| Error::InvalidInput(e.to_string()))?,
            )
            .map_err(|e| Error::InvalidInput(e.to_string()))?,
        );

        Ok(())
    }
    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn created_by(&self) -> Option<AgentId> {
        self.created_by
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskWithContext {
    #[serde(flatten)]
    pub task: Task,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ancestors: Vec<Task>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Task>,
}

pub struct RestoreTask {
    pub id: TaskId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub parent_id: Option<TaskId>,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub assigned_to: Option<AgentId>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub depends_on: Vec<TaskId>,
    pub tags: Vec<String>,
    pub result_summary: Option<String>,
    pub notes: Vec<Note>,
    pub created_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SubtaskDef {
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub depends_on: Vec<TaskId>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<ProjectId>,
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub assigned_to: Option<AgentId>,
    pub parent_id: Option<TaskId>,
    pub tag: Option<String>,
}

pub trait WatcherStore: Send + Sync {
    fn save(&self, watcher: &mut TaskWatcher) -> impl Future<Output = Result<()>> + Send;
    fn delete(
        &self,
        task_id: &TaskId,
        agent_id: &AgentId,
    ) -> impl Future<Output = Result<()>> + Send;
    fn find_watchers(
        &self,
        task_id: &TaskId,
    ) -> impl Future<Output = Result<Vec<TaskWatcher>>> + Send;
    fn find_by_agent(
        &self,
        agent_id: &AgentId,
    ) -> impl Future<Output = Result<Vec<TaskWatcher>>> + Send;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskWatcher {
    task_id: TaskId,
    agent_id: AgentId,
    project: ProjectId,
    namespace: Namespace,
    created_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl TaskWatcher {
    pub fn new(
        task_id: TaskId,
        agent_id: AgentId,
        project: ProjectId,
        namespace: Namespace,
    ) -> Self {
        let mut watcher = Self {
            task_id,
            agent_id,
            project,
            namespace,
            created_at: Utc::now(),
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            watcher.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_WATCHER_ADDED,
            Payload::from_json(&task_events::TaskWatcherAddedPayload {
                task_id: watcher.task_id.to_string(),
                agent_id: watcher.agent_id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| watcher.collector.collect(e));

        watcher
    }

    pub fn restore(
        task_id: TaskId,
        agent_id: AgentId,
        project: ProjectId,
        namespace: Namespace,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            task_id,
            agent_id,
            project,
            namespace,
            created_at,
            collector: EventCollector::new(),
        }
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn agent_id(&self) -> AgentId {
        self.agent_id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
}

impl fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReviewStatus::Pending => write!(f, "pending"),
            ReviewStatus::Approved => write!(f, "approved"),
            ReviewStatus::Rejected => write!(f, "rejected"),
        }
    }
}

impl FromStr for ReviewStatus {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "pending" => Ok(ReviewStatus::Pending),
            "approved" => Ok(ReviewStatus::Approved),
            "rejected" => Ok(ReviewStatus::Rejected),
            other => Err(format!("unknown review status: {other}")),
        }
    }
}

pub trait ReviewStore: Send + Sync {
    fn save(&self, review: &mut ReviewRequest) -> impl Future<Output = Result<()>> + Send;
    fn find_by_id(
        &self,
        id: &ReviewId,
    ) -> impl Future<Output = Result<Option<ReviewRequest>>> + Send;
    fn find_pending_for_agent(
        &self,
        agent_id: &AgentId,
    ) -> impl Future<Output = Result<Vec<ReviewRequest>>> + Send;
    fn find_by_task(
        &self,
        task_id: &TaskId,
    ) -> impl Future<Output = Result<Vec<ReviewRequest>>> + Send;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ReviewId(Uuid);

impl ReviewId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for ReviewId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ReviewId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for ReviewId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    id: ReviewId,
    task_id: TaskId,
    project: ProjectId,
    namespace: Namespace,
    requester: AgentId,
    reviewer: Option<AgentId>,
    reviewer_role: Option<String>,
    status: ReviewStatus,
    comments: Option<String>,
    created_at: DateTime<Utc>,
    resolved_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    collector: EventCollector,
}

impl ReviewRequest {
    pub fn new(
        task_id: TaskId,
        project: ProjectId,
        namespace: Namespace,
        requester: AgentId,
        reviewer: Option<AgentId>,
        reviewer_role: Option<String>,
    ) -> Self {
        let mut review = Self {
            id: ReviewId::new(),
            task_id,
            project,
            namespace,
            requester,
            reviewer,
            reviewer_role,
            status: ReviewStatus::Pending,
            comments: None,
            created_at: Utc::now(),
            resolved_at: None,
            collector: EventCollector::new(),
        };

        let _ = Event::create(
            review.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_REVIEW_REQUESTED,
            Payload::from_json(&task_events::ReviewRequestedPayload {
                review_id: review.id.to_string(),
                task_id: review.task_id.to_string(),
                requester: review.requester.to_string(),
            })
            .unwrap(),
        )
        .map(|e| review.collector.collect(e));

        review
    }

    pub fn restore(r: RestoreReviewRequest) -> Self {
        Self {
            id: r.id,
            task_id: r.task_id,
            project: r.project,
            namespace: r.namespace,
            requester: r.requester,
            reviewer: r.reviewer,
            reviewer_role: r.reviewer_role,
            status: r.status,
            comments: r.comments,
            created_at: r.created_at,
            resolved_at: r.resolved_at,
            collector: EventCollector::new(),
        }
    }

    pub fn approve(&mut self, comments: Option<String>) -> Result<()> {
        if self.status != ReviewStatus::Pending {
            return Err(Error::InvalidTransition {
                from: self.status.to_string(),
                to: "approved".into(),
            });
        }
        self.status = ReviewStatus::Approved;
        self.comments = comments;
        self.resolved_at = Some(Utc::now());

        let _ = Event::create(
            self.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_REVIEW_APPROVED,
            Payload::from_json(&task_events::ReviewApprovedPayload {
                review_id: self.id.to_string(),
                task_id: self.task_id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));

        Ok(())
    }

    pub fn unassign_reviewer(&mut self) {
        self.reviewer = None;
    }

    pub fn reject(&mut self, comments: Option<String>) -> Result<()> {
        if self.status != ReviewStatus::Pending {
            return Err(Error::InvalidTransition {
                from: self.status.to_string(),
                to: "rejected".into(),
            });
        }
        self.status = ReviewStatus::Rejected;
        self.comments = comments;
        self.resolved_at = Some(Utc::now());

        let _ = Event::create(
            self.project.as_ref(),
            task_events::NAMESPACE,
            task_events::TOPIC_REVIEW_REJECTED,
            Payload::from_json(&task_events::ReviewRejectedPayload {
                review_id: self.id.to_string(),
                task_id: self.task_id.to_string(),
            })
            .unwrap(),
        )
        .map(|e| self.collector.collect(e));

        Ok(())
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn id(&self) -> ReviewId {
        self.id
    }
    pub fn task_id(&self) -> TaskId {
        self.task_id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn requester(&self) -> AgentId {
        self.requester
    }
    pub fn reviewer(&self) -> Option<AgentId> {
        self.reviewer
    }
    pub fn reviewer_role(&self) -> Option<&str> {
        self.reviewer_role.as_deref()
    }
    pub fn status(&self) -> ReviewStatus {
        self.status
    }
    pub fn comments(&self) -> Option<&str> {
        self.comments.as_deref()
    }
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub fn resolved_at(&self) -> Option<DateTime<Utc>> {
        self.resolved_at
    }
}

pub struct RestoreReviewRequest {
    pub id: ReviewId,
    pub task_id: TaskId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub requester: AgentId,
    pub reviewer: Option<AgentId>,
    pub reviewer_role: Option<String>,
    pub status: ReviewStatus,
    pub comments: Option<String>,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(status: TaskStatus, assigned_to: Option<AgentId>) -> Task {
        Task::restore(RestoreTask {
            id: TaskId::new(),
            project: ProjectId::try_from("test").unwrap(),
            namespace: Namespace::root(),
            parent_id: None,
            title: "Test Task".to_string(),
            description: "Test".to_string(),
            status,
            priority: Priority::default(),
            assigned_roles: vec!["tester".to_string()],
            assigned_to,
            assigned_at: None,
            depends_on: vec![],
            tags: vec![],
            result_summary: None,
            notes: Vec::new(),
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    #[test]
    fn valid_transitions() {
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Claimed));
        assert!(TaskStatus::Pending.can_transition_to(&TaskStatus::Blocked));
        assert!(TaskStatus::Blocked.can_transition_to(&TaskStatus::Pending));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::InProgress));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Failed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Completed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Failed));
    }

    #[test]
    fn invalid_transitions() {
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::InProgress));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Completed));
        assert!(!TaskStatus::Pending.can_transition_to(&TaskStatus::Failed));
        assert!(!TaskStatus::Completed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Failed.can_transition_to(&TaskStatus::Pending));
        assert!(!TaskStatus::Blocked.can_transition_to(&TaskStatus::Claimed));
    }

    #[test]
    fn claim_succeeds_from_pending() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.claim(agent).is_ok());
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.assigned_to(), Some(agent));
        assert!(task.assigned_at().is_some());
    }

    #[test]
    fn claim_fails_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        assert!(task.claim(agent).is_err());
    }

    #[test]
    fn start_succeeds_when_claimed_by_agent() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        assert!(task.start(&agent).is_ok());
        assert_eq!(task.status(), TaskStatus::InProgress);
    }

    #[test]
    fn start_fails_when_claimed_by_different_agent() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1));
        assert!(task.start(&agent2).is_err());
    }

    #[test]
    fn complete_succeeds_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        assert!(task.complete(Some("done".to_string())).is_ok());
        assert_eq!(task.status(), TaskStatus::Completed);
        assert_eq!(task.result_summary(), Some("done"));
    }

    #[test]
    fn complete_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.complete(None).is_err());
    }

    #[test]
    fn fail_succeeds_from_in_progress() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent));
        assert!(task.fail(Some("error".to_string())).is_ok());
        assert_eq!(task.status(), TaskStatus::Failed);
    }

    #[test]
    fn fail_succeeds_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        assert!(task.fail(None).is_ok());
        assert_eq!(task.status(), TaskStatus::Failed);
    }

    #[test]
    fn fail_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.fail(None).is_err());
    }

    #[test]
    fn release_succeeds_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        assert!(task.release().is_ok());
        assert_eq!(task.status(), TaskStatus::Pending);
        assert!(task.assigned_to().is_none());
    }

    #[test]
    fn release_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.release().is_err());
    }

    #[test]
    fn assign_succeeds_from_claimed() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1));
        assert!(task.assign(agent2).is_ok());
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.assigned_to(), Some(agent2));
    }

    #[test]
    fn assign_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.assign(AgentId::new()).is_err());
    }

    #[test]
    fn unblock_from_blocked() {
        let mut task = make_task(TaskStatus::Blocked, None);
        task.unblock().unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn unblock_noop_from_other_status() {
        let mut task = make_task(TaskStatus::Pending, None);
        task.unblock().unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn is_mergeable_for_valid_statuses() {
        assert!(TaskStatus::Pending.is_mergeable());
        assert!(TaskStatus::Blocked.is_mergeable());
        assert!(TaskStatus::Claimed.is_mergeable());
    }

    #[test]
    fn is_not_mergeable_for_terminal_or_active_statuses() {
        assert!(!TaskStatus::InProgress.is_mergeable());
        assert!(!TaskStatus::Completed.is_mergeable());
        assert!(!TaskStatus::Failed.is_mergeable());
        assert!(!TaskStatus::Cancelled.is_mergeable());
    }

    #[test]
    fn set_parent_id_updates_field() {
        let mut task = make_task(TaskStatus::Pending, None);
        let parent = TaskId::new();
        task.set_parent_id(Some(parent));
        assert_eq!(task.parent_id(), Some(parent));
    }

    #[test]
    fn replace_dependency_swaps_id() {
        let old_dep = TaskId::new();
        let new_dep = TaskId::new();
        let mut task = make_task(TaskStatus::Pending, None);
        task.add_dependency(old_dep).unwrap();
        task.replace_dependency(&old_dep, new_dep);
        assert!(!task.depends_on().contains(&old_dep));
        assert!(task.depends_on().contains(&new_dep));
    }

    #[test]
    fn replace_dependency_deduplicates() {
        let dep_a = TaskId::new();
        let dep_b = TaskId::new();
        let mut task = make_task(TaskStatus::Pending, None);
        task.add_dependency(dep_a).unwrap();
        task.add_dependency(dep_b).unwrap();
        task.replace_dependency(&dep_a, dep_b);
        assert_eq!(task.depends_on().len(), 1);
        assert!(task.depends_on().contains(&dep_b));
    }

    #[test]
    fn new_creates_pending_task() {
        let task = Task::new(
            ProjectId::try_from("test").unwrap(),
            Namespace::root(),
            None,
            "title".to_string(),
            "desc".to_string(),
            Priority::High,
            vec![],
            vec![],
            None,
            false,
        )
        .unwrap();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn new_creates_blocked_task() {
        let task = Task::new(
            ProjectId::try_from("test").unwrap(),
            Namespace::root(),
            None,
            "title".to_string(),
            "desc".to_string(),
            Priority::Normal,
            vec![],
            vec![TaskId::new()],
            None,
            true,
        )
        .unwrap();
        assert_eq!(task.status(), TaskStatus::Blocked);
    }
}
