use chrono::Utc;

use orchy_core::entities::{Skill, SkillFilter, WriteSkill};
use orchy_core::error::{Error, Result};
use orchy_core::store::SkillStore;
use orchy_core::value_objects::Namespace;

use crate::MemoryBackend;

impl SkillStore for MemoryBackend {
    async fn write(&self, cmd: WriteSkill) -> Result<Skill> {
        let now = Utc::now();
        let key = (cmd.namespace.to_string(), cmd.name.clone());

        let mut store = self.skills.write().map_err(|e| Error::Store(e.to_string()))?;

        let skill = if let Some(existing) = store.get(&key) {
            Skill {
                namespace: existing.namespace.clone(),
                name: existing.name.clone(),
                description: cmd.description,
                content: cmd.content,
                written_by: cmd.written_by.or(existing.written_by),
                created_at: existing.created_at,
                updated_at: now,
            }
        } else {
            Skill {
                namespace: cmd.namespace,
                name: cmd.name,
                description: cmd.description,
                content: cmd.content,
                written_by: cmd.written_by,
                created_at: now,
                updated_at: now,
            }
        };

        store.insert(key, skill.clone());
        Ok(skill)
    }

    async fn read(&self, namespace: &Namespace, name: &str) -> Result<Option<Skill>> {
        let store = self.skills.read().map_err(|e| Error::Store(e.to_string()))?;
        let key = (namespace.to_string(), name.to_string());
        Ok(store.get(&key).cloned())
    }

    async fn list(&self, filter: SkillFilter) -> Result<Vec<Skill>> {
        let store = self.skills.read().map_err(|e| Error::Store(e.to_string()))?;

        Ok(store
            .values()
            .filter(|skill| {
                if let Some(ref ns) = filter.namespace {
                    skill.namespace.starts_with(ns)
                } else {
                    true
                }
            })
            .cloned()
            .collect())
    }

    async fn delete(&self, namespace: &Namespace, name: &str) -> Result<()> {
        let mut store = self.skills.write().map_err(|e| Error::Store(e.to_string()))?;
        let key = (namespace.to_string(), name.to_string());
        store.remove(&key);
        Ok(())
    }
}
