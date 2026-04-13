pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::future::Future;
use std::str::FromStr;
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, ProjectId};
use crate::note::Note;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TaskId(Uuid);

impl TaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
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
}

impl TaskStatus {
    pub fn can_transition_to(&self, target: &TaskStatus) -> bool {
        use TaskStatus::*;
        matches!(
            (self, target),
            (Pending, Claimed)
                | (Pending, Blocked)
                | (Blocked, Pending)
                | (Claimed, InProgress)
                | (Claimed, Pending)
                | (Claimed, Failed)
                | (InProgress, Completed)
                | (InProgress, Failed)
                | (InProgress, Pending)
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
        };
        write!(f, "{s}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low,
    Normal,
    High,
    Critical,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Normal
    }
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
    namespace: Namespace,
    title: String,
    description: String,
    status: TaskStatus,
    priority: Priority,
    assigned_roles: Vec<String>,
    claimed_by: Option<AgentId>,
    claimed_at: Option<DateTime<Utc>>,
    depends_on: Vec<TaskId>,
    result_summary: Option<String>,
    notes: Vec<Note>,
    created_by: Option<AgentId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Task {
    pub fn new(
        namespace: Namespace,
        title: String,
        description: String,
        priority: Priority,
        assigned_roles: Vec<String>,
        depends_on: Vec<TaskId>,
        created_by: Option<AgentId>,
        is_blocked: bool,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: TaskId::new(),
            namespace,
            title,
            description,
            status: if is_blocked {
                TaskStatus::Blocked
            } else {
                TaskStatus::Pending
            },
            priority,
            assigned_roles,
            claimed_by: None,
            claimed_at: None,
            depends_on,
            result_summary: None,
            notes: Vec::new(),
            created_by,
            created_at: now,
            updated_at: now,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: TaskId,
        namespace: Namespace,
        title: String,
        description: String,
        status: TaskStatus,
        priority: Priority,
        assigned_roles: Vec<String>,
        claimed_by: Option<AgentId>,
        claimed_at: Option<DateTime<Utc>>,
        depends_on: Vec<TaskId>,
        result_summary: Option<String>,
        notes: Vec<Note>,
        created_by: Option<AgentId>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            namespace,
            title,
            description,
            status,
            priority,
            assigned_roles,
            claimed_by,
            claimed_at,
            depends_on,
            result_summary,
            notes,
            created_by,
            created_at,
            updated_at,
        }
    }

    pub fn claim(&mut self, agent: AgentId) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Claimed)?;
        self.claimed_by = Some(agent);
        self.claimed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn start(&mut self, agent: &AgentId) -> Result<()> {
        if self.claimed_by != Some(*agent) {
            return Err(Error::InvalidInput(format!(
                "task {} is not claimed by agent {}",
                self.id, agent
            )));
        }
        self.status = self.status.transition_to(TaskStatus::InProgress)?;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn complete(&mut self, summary: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Completed)?;
        self.result_summary = summary;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn fail(&mut self, reason: Option<String>) -> Result<()> {
        self.status = self.status.transition_to(TaskStatus::Failed)?;
        self.result_summary = reason;
        self.updated_at = Utc::now();
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
        self.claimed_by = None;
        self.claimed_at = None;
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn reassign(&mut self, new_agent: AgentId) -> Result<()> {
        if !matches!(self.status, TaskStatus::Claimed | TaskStatus::InProgress) {
            return Err(Error::InvalidInput(format!(
                "task {} cannot be reassigned from status {}",
                self.id, self.status
            )));
        }
        self.status = TaskStatus::Claimed;
        self.claimed_by = Some(new_agent);
        self.claimed_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }

    pub fn unblock(&mut self) {
        if self.status == TaskStatus::Blocked {
            self.status = TaskStatus::Pending;
            self.updated_at = Utc::now();
        }
    }

    pub fn id(&self) -> TaskId {
        self.id
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
    pub fn status(&self) -> TaskStatus {
        self.status
    }
    pub fn priority(&self) -> Priority {
        self.priority
    }
    pub fn assigned_roles(&self) -> &[String] {
        &self.assigned_roles
    }
    pub fn claimed_by(&self) -> Option<AgentId> {
        self.claimed_by
    }
    pub fn claimed_at(&self) -> Option<DateTime<Utc>> {
        self.claimed_at
    }
    pub fn depends_on(&self) -> &[TaskId] {
        &self.depends_on
    }
    pub fn result_summary(&self) -> Option<&str> {
        self.result_summary.as_deref()
    }
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }
    pub fn add_note(&mut self, author: Option<AgentId>, body: String) {
        self.notes.push(Note::new(author, body));
        self.updated_at = Utc::now();
    }
    pub fn move_to(&mut self, namespace: Namespace) {
        self.namespace = namespace;
        self.updated_at = Utc::now();
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

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<ProjectId>,
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub claimed_by: Option<AgentId>,
}

pub trait TaskStore: Send + Sync {
    fn save(&self, task: &Task) -> impl Future<Output = Result<()>> + Send;
    fn get(&self, id: &TaskId) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn list(&self, filter: TaskFilter) -> impl Future<Output = Result<Vec<Task>>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(status: TaskStatus, claimed_by: Option<AgentId>) -> Task {
        Task::restore(
            TaskId::new(),
            Namespace::try_from("test".to_string()).unwrap(),
            "Test Task".to_string(),
            "Test".to_string(),
            status,
            Priority::default(),
            vec!["tester".to_string()],
            claimed_by,
            None,
            vec![],
            None,
            Vec::new(),
            None,
            Utc::now(),
            Utc::now(),
        )
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
        assert_eq!(task.claimed_by(), Some(agent));
        assert!(task.claimed_at().is_some());
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
        assert!(task.claimed_by().is_none());
    }

    #[test]
    fn release_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.release().is_err());
    }

    #[test]
    fn reassign_succeeds_from_claimed() {
        let agent1 = AgentId::new();
        let agent2 = AgentId::new();
        let mut task = make_task(TaskStatus::Claimed, Some(agent1));
        assert!(task.reassign(agent2).is_ok());
        assert_eq!(task.status(), TaskStatus::Claimed);
        assert_eq!(task.claimed_by(), Some(agent2));
    }

    #[test]
    fn reassign_fails_from_pending() {
        let mut task = make_task(TaskStatus::Pending, None);
        assert!(task.reassign(AgentId::new()).is_err());
    }

    #[test]
    fn unblock_from_blocked() {
        let mut task = make_task(TaskStatus::Blocked, None);
        task.unblock();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn unblock_noop_from_other_status() {
        let mut task = make_task(TaskStatus::Pending, None);
        task.unblock();
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn new_creates_pending_task() {
        let task = Task::new(
            Namespace::try_from("test".to_string()).unwrap(),
            "title".to_string(),
            "desc".to_string(),
            Priority::High,
            vec![],
            vec![],
            None,
            false,
        );
        assert_eq!(task.status(), TaskStatus::Pending);
    }

    #[test]
    fn new_creates_blocked_task() {
        let task = Task::new(
            Namespace::try_from("test".to_string()).unwrap(),
            "title".to_string(),
            "desc".to_string(),
            Priority::Normal,
            vec![],
            vec![TaskId::new()],
            None,
            true,
        );
        assert_eq!(task.status(), TaskStatus::Blocked);
    }
}
