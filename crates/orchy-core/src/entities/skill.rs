use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::value_objects::{AgentId, Namespace, Project};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub namespace: Namespace,
    pub name: String,
    pub description: String,
    pub content: String,
    pub written_by: Option<AgentId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WriteSkill {
    pub namespace: Namespace,
    pub name: String,
    pub description: String,
    pub content: String,
    pub written_by: Option<AgentId>,
}

#[derive(Debug, Clone, Default)]
pub struct SkillFilter {
    pub namespace: Option<Namespace>,
    pub project: Option<Project>,
}
