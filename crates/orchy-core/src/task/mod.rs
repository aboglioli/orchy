pub mod events;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use orchy_events::{Event, EventCollector, Payload};

use self::events as task_events;
use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::organization::OrganizationId;
use crate::pagination::{Page, PageParams};

#[async_trait::async_trait]
pub trait TaskStore: Send + Sync {
    async fn save(&self, task: &mut Task) -> Result<()>;
    async fn find_by_id(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn find_by_ids(&self, ids: &[TaskId]) -> Result<Vec<Task>>;
    async fn list(&self, filter: TaskFilter, page: PageParams) -> Result<Page<Task>>;
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
    type Err = Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| Error::invalid_input(format!("invalid task id: {s}")))
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
                | (Claimed, Completed)
                | (Claimed, Blocked)
                | (Claimed, Pending)
                | (Claimed, Failed)
                | (Claimed, Cancelled)
                | (InProgress, Claimed)
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
    org_id: OrganizationId,
    project: ProjectId,
    namespace: Namespace,
    title: String,
    description: String,
    acceptance_criteria: Option<String>,
    status: TaskStatus,
    priority: Priority,
    assigned_roles: Vec<String>,
    assigned_to: Option<AgentId>,
    assigned_at: Option<DateTime<Utc>>,
    stale_after_secs: Option<u64>,
    last_activity_at: DateTime<Utc>,
    tags: Vec<String>,
    result_summary: Option<String>,
    created_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    collector: EventCollector,
}

impl Task {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        org_id: OrganizationId,
        project: ProjectId,
        namespace: Namespace,
        title: String,
        description: String,
        acceptance_criteria: Option<String>,
        priority: Priority,
        assigned_roles: Vec<String>,
        created_by: Option<AgentId>,
        is_blocked: bool,
    ) -> Result<Self> {
        if title.trim().is_empty() {
            return Err(Error::InvalidInput("task title must not be empty".into()));
        }

        let now = Utc::now();
        let mut task = Self {
            id: TaskId::new(),
            org_id,
            project,
            namespace,
            title,
            description,
            acceptance_criteria,
            status: if is_blocked {
                TaskStatus::Blocked
            } else {
                TaskStatus::Pending
            },
            priority,
            assigned_roles,
            assigned_to: None,
            assigned_at: None,
            stale_after_secs: None,
            last_activity_at: now,
            tags: Vec::new(),
            result_summary: None,
            created_by,
            created_at: now,
            updated_at: now,
            collector: EventCollector::new(),
        };

        task.collector.collect(
            Event::create(
                task.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_CREATED,
                Payload::from_json(&task_events::TaskCreatedPayload {
                    org_id: task.org_id.to_string(),
                    task_id: task.id.to_string(),
                    project: task.project.to_string(),
                    namespace: task.namespace.to_string(),
                    title: task.title.clone(),
                    description: task.description.clone(),
                    acceptance_criteria: task.acceptance_criteria.clone(),
                    priority: task.priority.to_string(),
                    assigned_roles: task.assigned_roles.clone(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(task)
    }

    pub fn restore(r: RestoreTask) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            project: r.project,
            namespace: r.namespace,
            title: r.title,
            description: r.description,
            acceptance_criteria: r.acceptance_criteria,
            status: r.status,
            priority: r.priority,
            assigned_roles: r.assigned_roles,
            assigned_to: r.assigned_to,
            assigned_at: r.assigned_at,
            stale_after_secs: r.stale_after_secs,
            last_activity_at: r.last_activity_at,
            tags: r.tags,
            result_summary: r.result_summary,
            created_by: r.created_by,
            created_at: r.created_at,
            updated_at: r.updated_at,
            collector: EventCollector::new(),
        }
    }

    pub fn claim(&mut self, agent: AgentId) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Claimed)?;
        self.assigned_to = Some(agent.clone());
        self.assigned_at = Some(Utc::now());
        self.updated_at = Utc::now();
        self.touch();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_CLAIMED,
                Payload::from_json(&task_events::TaskClaimedPayload {
                    task_id: self.id.to_string(),
                    agent_id: agent.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn start(&mut self, agent: &AgentId) -> Result<()> {
        if self.assigned_to.as_ref() != Some(agent) {
            return Err(Error::InvalidInput(format!(
                "task {} is not claimed by agent {}",
                self.id, agent
            )));
        }
        self.status = self.status.transition_to(TaskStatus::InProgress)?;
        self.updated_at = Utc::now();
        self.touch();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_STARTED,
                Payload::from_json(&task_events::TaskStartedPayload {
                    task_id: self.id.to_string(),
                    agent_id: agent.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn complete(&mut self, summary: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Completed)?;
        self.result_summary = summary.clone();
        self.updated_at = Utc::now();
        self.touch();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_COMPLETED,
                Payload::from_json(&task_events::TaskCompletedPayload {
                    task_id: self.id.to_string(),
                    summary,
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn auto_complete(&mut self, summary: String) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Completed)?;
        self.result_summary = Some(summary.clone());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_AUTO_COMPLETED,
                Payload::from_json(&task_events::TaskCompletedPayload {
                    task_id: self.id.to_string(),
                    summary: Some(summary),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn fail(&mut self, reason: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Failed)?;
        self.result_summary = reason.clone();
        self.updated_at = Utc::now();
        self.touch();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_FAILED,
                Payload::from_json(&task_events::TaskFailedPayload {
                    task_id: self.id.to_string(),
                    reason,
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
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
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_RELEASED,
                Payload::from_json(&task_events::TaskReleasedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
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
        self.assigned_to = Some(new_agent.clone());
        self.assigned_at = Some(Utc::now());
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_ASSIGNED,
                Payload::from_json(&task_events::TaskAssignedPayload {
                    task_id: self.id.to_string(),
                    agent_id: new_agent.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
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
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_BLOCKED,
                Payload::from_json(&task_events::TaskBlockedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn unblock(&mut self) -> Result<()> {
        if self.status != TaskStatus::Blocked {
            return Ok(());
        }
        self.status = self.status.transition_to(TaskStatus::Pending)?;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_UNBLOCKED,
                Payload::from_json(&task_events::TaskUnblockedPayload {
                    task_id: self.id.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn cancel(&mut self, reason: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Cancelled)?;
        self.result_summary = reason.clone();
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_CANCELLED,
                Payload::from_json(&task_events::TaskCancelledPayload {
                    task_id: self.id.to_string(),
                    reason,
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn update_details(
        &mut self,
        title: Option<String>,
        description: Option<String>,
        acceptance_criteria: Option<String>,
        priority: Option<Priority>,
    ) -> Result<()> {
        if title.is_none()
            && description.is_none()
            && acceptance_criteria.is_none()
            && priority.is_none()
        {
            return Err(Error::InvalidInput("no task fields to update".into()));
        }
        if matches!(
            self.status,
            TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
        ) {
            return Err(Error::InvalidInput(format!(
                "cannot update task in status {}",
                self.status
            )));
        }
        let mut new_title = None;
        let mut new_description = None;
        let mut new_acceptance_criteria = None;
        let mut new_priority = None;
        if let Some(t) = title {
            if t.trim().is_empty() {
                return Err(Error::InvalidInput("task title must not be empty".into()));
            }
            new_title = Some(t.clone());
            self.title = t;
        }
        if let Some(d) = description {
            new_description = Some(d.clone());
            self.description = d;
        }
        if let Some(a) = acceptance_criteria {
            new_acceptance_criteria = Some(a.clone());
            self.acceptance_criteria = Some(a);
        }
        if let Some(p) = priority {
            new_priority = Some(p.to_string());
            self.priority = p;
        }
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_UPDATED,
                Payload::from_json(&task_events::TaskUpdatedPayload {
                    task_id: self.id.to_string(),
                    title: new_title,
                    description: new_description,
                    acceptance_criteria: new_acceptance_criteria,
                    priority: new_priority,
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn id(&self) -> TaskId {
        self.id
    }
    pub fn org_id(&self) -> &OrganizationId {
        &self.org_id
    }
    pub fn project(&self) -> &ProjectId {
        &self.project
    }
    pub fn namespace(&self) -> &Namespace {
        &self.namespace
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn acceptance_criteria(&self) -> Option<&str> {
        self.acceptance_criteria.as_deref()
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
    pub fn assigned_to(&self) -> Option<&AgentId> {
        self.assigned_to.as_ref()
    }
    pub fn assigned_at(&self) -> Option<DateTime<Utc>> {
        self.assigned_at
    }
    pub fn stale_after_secs(&self) -> Option<u64> {
        self.stale_after_secs
    }
    pub fn last_activity_at(&self) -> DateTime<Utc> {
        self.last_activity_at
    }
    pub fn is_stale(&self) -> bool {
        match self.stale_after_secs {
            None => false,
            Some(secs) => {
                let elapsed = (Utc::now() - self.last_activity_at).num_seconds() as u64;
                elapsed > secs
            }
        }
    }
    pub fn touch(&mut self) {
        self.last_activity_at = Utc::now();
        self.updated_at = Utc::now();
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
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_TAGGED,
                Payload::from_json(&task_events::TaskTaggedPayload {
                    task_id: self.id.to_string(),
                    tag,
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
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
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_TAG_REMOVED,
                Payload::from_json(&task_events::TaskTagRemovedPayload {
                    task_id: self.id.to_string(),
                    tag: tag.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }
    pub fn result_summary(&self) -> Option<&str> {
        self.result_summary.as_deref()
    }
    pub fn move_to(&mut self, namespace: Namespace) -> Result<()> {
        let from_namespace = self.namespace.to_string();
        self.namespace = namespace;
        self.updated_at = Utc::now();

        self.collector.collect(
            Event::create(
                self.org_id.as_str(),
                task_events::NAMESPACE,
                task_events::TOPIC_MOVED,
                Payload::from_json(&task_events::TaskMovedPayload {
                    task_id: self.id.to_string(),
                    from_namespace,
                    to_namespace: self.namespace.to_string(),
                })
                .map_err(|e| Error::Store(format!("event creation: {e}")))?,
            )
            .map_err(|e| Error::Store(format!("event creation: {e}")))?,
        );

        Ok(())
    }

    pub fn all_children_completed(children: &[Task]) -> bool {
        !children.is_empty()
            && children
                .iter()
                .all(|c| matches!(c.status(), TaskStatus::Completed | TaskStatus::Cancelled))
    }

    pub fn drain_events(&mut self) -> Vec<Event> {
        self.collector.drain()
    }

    pub fn created_by(&self) -> Option<&AgentId> {
        self.created_by.as_ref()
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
    pub org_id: OrganizationId,
    pub project: ProjectId,
    pub namespace: Namespace,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub status: TaskStatus,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub assigned_to: Option<AgentId>,
    pub assigned_at: Option<DateTime<Utc>>,
    pub stale_after_secs: Option<u64>,
    pub last_activity_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub result_summary: Option<String>,
    pub created_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SubtaskDef {
    pub title: String,
    pub description: String,
    pub acceptance_criteria: Option<String>,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub org_id: Option<OrganizationId>,
    pub namespace: Option<Namespace>,
    pub project: Option<ProjectId>,
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub assigned_to: Option<AgentId>,
    pub tag: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(status: TaskStatus, assigned_to: Option<AgentId>) -> Task {
        use orchy_events::OrganizationId;
        Task::restore(RestoreTask {
            id: TaskId::new(),
            org_id: OrganizationId::new("test").unwrap(),
            project: ProjectId::try_from("test").unwrap(),
            namespace: Namespace::root(),
            title: "Test Task".to_string(),
            description: "Test".to_string(),
            acceptance_criteria: None,
            status,
            priority: Priority::default(),
            assigned_roles: vec!["tester".to_string()],
            assigned_to,
            assigned_at: None,
            stale_after_secs: None,
            last_activity_at: Utc::now(),
            tags: vec![],
            result_summary: None,
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
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Completed));
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Failed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Claimed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Completed));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Failed));
    }

    #[test]
    fn auto_complete_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent));
        assert!(task.auto_complete("all children done".to_string()).is_ok());
        assert_eq!(task.status(), TaskStatus::Completed);
        assert_eq!(task.result_summary(), Some("all children done"));
    }

    #[test]
    fn assign_preserves_in_progress_status() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::InProgress, Some(agent1));
        assert!(task.assign(agent2.clone()).is_ok());
        assert_eq!(task.status(), TaskStatus::InProgress);
        assert_eq!(task.assigned_to(), Some(agent2).as_ref());
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
        assert!(task.claim(agent.clone()).is_ok());
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.assigned_to(), Some(agent).as_ref());
        assert!(task.assigned_at().is_some());
    }

    #[test]
    fn claim_fails_from_claimed() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent.clone()));
        assert!(task.claim(agent).is_err());
    }

    #[test]
    fn start_succeeds_when_claimed_by_agent() {
        let agent = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent.clone()));
        assert!(task.start(&agent).is_ok());
        assert_eq!(task.status(), TaskStatus::InProgress);
    }

    #[test]
    fn start_fails_when_claimed_by_different_agent() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1.clone()));
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
    fn acceptance_criteria_is_set_and_updatable() {
        let mut task = Task::new(
            OrganizationId::new("test").unwrap(),
            ProjectId::try_from("test").unwrap(),
            Namespace::root(),
            "Task with criteria".to_string(),
            "Implement feature".to_string(),
            Some("all tests pass and docs updated".to_string()),
            Priority::default(),
            vec!["engineer".to_string()],
            None,
            false,
        )
        .unwrap();

        assert_eq!(
            task.acceptance_criteria(),
            Some("all tests pass and docs updated")
        );

        task.update_details(
            None,
            None,
            Some("tests pass and integration verified".to_string()),
            None,
        )
        .unwrap();

        assert_eq!(
            task.acceptance_criteria(),
            Some("tests pass and integration verified")
        );
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
        let mut task = make_task(TaskStatus::Claimed, Some(agent1.clone()));
        assert!(task.assign(agent2.clone()).is_ok());
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.assigned_to(), Some(agent2).as_ref());
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
    fn new_creates_pending_task() {
        let task = Task::new(
            OrganizationId::new("test").unwrap(),
            ProjectId::try_from("test").unwrap(),
            Namespace::root(),
            "title".to_string(),
            "desc".to_string(),
            None,
            Priority::High,
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
            OrganizationId::new("test").unwrap(),
            ProjectId::try_from("test").unwrap(),
            Namespace::root(),
            "title".to_string(),
            "desc".to_string(),
            None,
            Priority::Normal,
            vec![],
            None,
            true,
        )
        .unwrap();
        assert_eq!(task.status(), TaskStatus::Blocked);
    }

    #[test]
    fn all_children_completed_requires_nonempty() {
        assert!(!Task::all_children_completed(&[]));
    }

    #[test]
    fn all_children_completed_when_all_completed() {
        let children = vec![
            make_task(TaskStatus::Completed, None),
            make_task(TaskStatus::Completed, None),
        ];
        assert!(Task::all_children_completed(&children));
    }

    #[test]
    fn all_children_completed_when_all_cancelled() {
        let children = vec![
            make_task(TaskStatus::Cancelled, None),
            make_task(TaskStatus::Cancelled, None),
        ];
        assert!(Task::all_children_completed(&children));
    }

    #[test]
    fn all_children_completed_with_mixed_completed_and_cancelled() {
        let children = vec![
            make_task(TaskStatus::Completed, None),
            make_task(TaskStatus::Cancelled, None),
        ];
        assert!(Task::all_children_completed(&children));
    }

    #[test]
    fn all_children_completed_false_when_any_pending() {
        let children = vec![
            make_task(TaskStatus::Completed, None),
            make_task(TaskStatus::Pending, None),
        ];
        assert!(!Task::all_children_completed(&children));
    }
}
