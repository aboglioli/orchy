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

    pub async fn write(&self, cmd: WriteSkill) -> Result<Skill> {
        if cmd.name.is_empty() {
            return Err(Error::InvalidInput(
                "skill name must not be empty".to_string(),
            ));
        }

        let skill =
            if let Some(mut existing) = self.store.find_by_name(&cmd.namespace, &cmd.name).await? {
                existing.update(cmd.description, cmd.content, cmd.written_by);
                existing
            } else {
                Skill::new(
                    cmd.namespace,
                    cmd.name,
                    cmd.description,
                    cmd.content,
                    cmd.written_by,
                )
            };

        self.store.save(&skill).await?;
        Ok(skill)
    }

    pub async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        self.store.find_by_name(namespace, name).await
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
