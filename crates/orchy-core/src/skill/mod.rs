pub mod aggregate;
pub mod service;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent::AgentId;
use crate::error::Result;
use crate::namespace::{Namespace, Project};

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

pub trait SkillStore: Send + Sync {
    async fn write(&self, skill: WriteSkill) -> Result<Skill>;
    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>>;
    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>>;
    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()>;
}
