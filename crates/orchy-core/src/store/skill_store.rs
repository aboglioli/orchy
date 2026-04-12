use crate::entities::{Skill, SkillFilter, WriteSkill};
use crate::error::Result;
use crate::value_objects::Namespace;

pub trait SkillStore: Send + Sync {
    async fn write(&self, skill: WriteSkill) -> Result<Skill>;
    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>>;
    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>>;
    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()>;
}
