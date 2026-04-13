use std::sync::Arc;

use super::{Skill, SkillFilter, SkillStore, WriteSkill};
use crate::error::{Error, Result};
use crate::namespace::Namespace;

pub struct SkillService<S: SkillStore> {
    store: Arc<S>,
}

impl<S: SkillStore> SkillService<S> {
    pub fn new(store: Arc<S>) -> Self {
        Self { store }
    }

    pub async fn write(&self, skill: WriteSkill) -> Result<Skill> {
        if skill.name.is_empty() {
            return Err(Error::InvalidInput(
                "skill name must not be empty".to_string(),
            ));
        }
        self.store.write(skill).await
    }

    pub async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        self.store.read(namespace, name).await
    }

    pub async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        self.store.list(filter).await
    }

    pub async fn list_with_inherited(&self, namespace: &Namespace) -> Result<Vec<Skill>> {
        let all = self
            .store
            .list(SkillFilter {
                namespace: Some(Namespace::try_from(namespace.project().to_string()).unwrap()),
                ..Default::default()
            })
            .await?;

        Ok(Skill::filter_with_inheritance(all, namespace))
    }

    pub async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        self.store.delete(namespace, name).await
    }
}
