use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    EntityKind, EntityRef, Hit, Passage, Result, Search, SearchQuery, SkillStore, score,
};

use crate::documents::MemoryDocumentStore;
use crate::skills::MemorySkillStore;

const EXCERPT: usize = 240;

pub struct MemorySearch {
    documents: Arc<MemoryDocumentStore>,
    skills: Arc<MemorySkillStore>,
}

impl MemorySearch {
    pub fn new(documents: Arc<MemoryDocumentStore>, skills: Arc<MemorySkillStore>) -> Self {
        Self { documents, skills }
    }
}

#[async_trait]
impl Search for MemorySearch {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>> {
        let mut passages = Vec::new();

        if query.covers(EntityKind::Skill) {
            for skill in self.skills.all().await? {
                if !query.retired && !skill.is_active() {
                    continue;
                }
                passages.push(Passage {
                    entity: EntityRef::new(EntityKind::Skill, skill.id().clone()),
                    heading: Some(skill.name().to_string()),
                    title: format!("{} {}", skill.name(), skill.summary()),
                    body: skill.body().as_str().to_owned(),
                    excerpt: skill.summary().to_string(),
                    namespace: skill.namespace().clone(),
                    updated_at: skill.updated_at(),
                });
            }
        }
        if !query.covers(EntityKind::Document) {
            return Ok(score(passages, &query.text));
        }

        for document in self.documents.snapshot() {
            if let Some(kinds) = &query.kind
                && !kinds.contains(document.kind())
            {
                continue;
            }
            if let Some(statuses) = &query.status {
                match document.status() {
                    Some(status) if statuses.contains(&status) => {}
                    _ => continue,
                }
            }
            if let Some(namespace) = &query.namespace
                && !namespace.contains(document.namespace())
            {
                continue;
            }
            if !query.tags.iter().all(|t| document.tags().contains(t)) {
                continue;
            }

            let body = document.body().as_str();
            passages.push(Passage {
                entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                heading: None,
                title: document.title().to_string(),
                body: body.to_owned(),
                excerpt: body.trim().chars().take(EXCERPT).collect(),
                namespace: document.namespace().clone(),
                updated_at: document.updated_at(),
            });
        }
        Ok(score(passages, &query.text))
    }
}
