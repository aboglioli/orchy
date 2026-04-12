pub mod aggregate;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::agent::AgentId;
use crate::error::{Error, Result};
use crate::namespace::{Namespace, Project};

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

    pub fn transition_to(&self, target: TaskStatus) -> Result<TaskStatus> {
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
    pub id: TaskId,
    pub namespace: Namespace,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub claimed_by: Option<AgentId>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub depends_on: Vec<TaskId>,
    pub result_summary: Option<String>,
    pub created_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateTask {
    pub namespace: Namespace,
    pub title: String,
    pub description: String,
    pub priority: Priority,
    pub assigned_roles: Vec<String>,
    pub depends_on: Vec<TaskId>,
    pub created_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<Project>,
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub claimed_by: Option<AgentId>,
}

pub trait TaskStore: Send + Sync {
    async fn create(&self, task: CreateTask) -> Result<Task>;
    async fn get(&self, id: &TaskId) -> Result<Option<Task>>;
    async fn list(&self, filter: TaskFilter) -> Result<Vec<Task>>;
    async fn claim(&self, id: &TaskId, agent: &AgentId) -> Result<Task>;
    async fn complete(&self, id: &TaskId, summary: Option<String>) -> Result<Task>;
    async fn fail(&self, id: &TaskId, reason: Option<String>) -> Result<Task>;
    async fn release(&self, id: &TaskId) -> Result<Task>;
    async fn update(&self, task: &Task) -> Result<Task>;
    async fn update_status(&self, id: &TaskId, status: TaskStatus) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn release_transitions() {
        assert!(TaskStatus::Claimed.can_transition_to(&TaskStatus::Pending));
        assert!(TaskStatus::InProgress.can_transition_to(&TaskStatus::Pending));

        let result = TaskStatus::Claimed.transition_to(TaskStatus::Pending);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TaskStatus::Pending);

        let result = TaskStatus::InProgress.transition_to(TaskStatus::Pending);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), TaskStatus::Pending);
    }
}
