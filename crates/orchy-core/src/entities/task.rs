use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, Namespace, Priority, TaskId, TaskStatus};

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
    pub status: Option<TaskStatus>,
    pub assigned_role: Option<String>,
    pub claimed_by: Option<AgentId>,
}
