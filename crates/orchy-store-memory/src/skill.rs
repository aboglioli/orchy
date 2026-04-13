use orchy_core::error::{Error, Result};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::skill::{Skill, SkillFilter, SkillStore};

use crate::MemoryBackend;

impl SkillStore for MemoryBackend {
    async fn save(&self, skill: &Skill) -> Result<()> {
        let key = (
            skill.project().to_string(),
            skill.namespace().to_string(),
            skill.name().to_string(),
        );

        let mut store = self
            .skills
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;

        store.insert(key, skill.clone());
        Ok(())
    }

    async fn find_by_name(
        &self,
        project: &ProjectId,
        namespace: &Namespace,
        name: &str,
    ) -> Result<Option<Skill>> {
        let store = self
            .skills
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;
        let key = (project.to_string(), namespace.to_string(), name.to_string());
        Ok(store.get(&key).cloned())
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let store = self
            .skills
            .read()
            .map_err(|e| Error::Store(e.to_string()))?;

        Ok(store
            .values()
            .filter(|skill| {
                if let Some(ref ns) = filter.namespace {
                    if !skill.namespace().starts_with(ns) {
                        return false;
                    }
                }
                if let Some(ref project) = filter.project {
                    if skill.project() != project {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect())
    }

    async fn delete(&self, project: &ProjectId, namespace: &Namespace, name: &str) -> Result<()> {
        let mut store = self
            .skills
            .write()
            .map_err(|e| Error::Store(e.to_string()))?;
        let key = (project.to_string(), namespace.to_string(), name.to_string());
        store.remove(&key);
        Ok(())
    }
}
