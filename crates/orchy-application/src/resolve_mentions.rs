use std::sync::Arc;

use orchy_core::{Document, DocumentStore, EntityKind, EntityRef, SkillStore, TaskStore};

use crate::error::ApplicationResult;

/// Wikilinks are display-only links: resolved when read, never stored as edges, and one that
/// names nothing in the vault is dropped.
pub struct ResolveMentions {
    documents: Arc<dyn DocumentStore>,
    tasks: Arc<dyn TaskStore>,
    skills: Arc<dyn SkillStore>,
}

impl ResolveMentions {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        tasks: Arc<dyn TaskStore>,
        skills: Arc<dyn SkillStore>,
    ) -> Self {
        Self {
            documents,
            tasks,
            skills,
        }
    }

    pub async fn execute(&self, document: &Document) -> ApplicationResult<Vec<EntityRef>> {
        let mut resolved = Vec::new();
        for id in document.body().mentions() {
            if self.documents.get(&id).await?.is_some() {
                resolved.push(EntityRef::document(id));
            } else if self.tasks.get(&id).await?.is_some() {
                resolved.push(EntityRef::task(id));
            } else if self.skills.get(&id).await?.is_some() {
                resolved.push(EntityRef::new(EntityKind::Skill, id));
            }
        }
        Ok(resolved)
    }
}
