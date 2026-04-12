use std::sync::Arc;

use crate::domain::SkillAggregate;
use crate::entities::{Skill, SkillFilter, WriteSkill};
use crate::error::{Error, Result};
use crate::store::Store;
use crate::value_objects::Namespace;

pub struct SkillService<S: Store> {
    store: Arc<S>,
}

impl<S: Store> SkillService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn write(&self, skill: WriteSkill) -> Result<Skill> {
        if skill.name.is_empty() {
            return Err(Error::InvalidInput(
                "skill name must not be empty".to_string(),
            ));
        }
        self.store.write_skill(skill).await
    }

    pub async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        self.store.read_skill(namespace, name).await
    }

    pub async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        self.store.list_skills(filter).await
    }

    pub async fn list_with_inherited(&self, namespace: &Namespace) -> Result<Vec<Skill>> {
        let all = self
            .store
            .list_skills(SkillFilter {
                namespace: Some(Namespace::try_from(namespace.project().to_string()).unwrap()),
            })
            .await?;

        Ok(SkillAggregate::filter_with_inheritance(all, namespace))
    }

    pub async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        self.store.delete_skill(namespace, name).await
    }
}
