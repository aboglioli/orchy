use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{EntityKind, EntityRef, Hit, Result, Search, SearchQuery, SkillStore};

use crate::documents::MemoryDocumentStore;
use crate::skills::MemorySkillStore;

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
        let needle = query.text.to_lowercase();
        let mut hits = Vec::new();

        if query.covers(EntityKind::Skill) {
            for skill in self.skills.all().await? {
                if !query.retired && !skill.is_active() {
                    continue;
                }
                let haystack = format!(
                    "{} {} {}",
                    skill.name(),
                    skill.summary(),
                    skill.body().as_str()
                )
                .to_lowercase();
                let matches = haystack.matches(&needle).count();
                if matches == 0 && !needle.is_empty() {
                    continue;
                }
                hits.push(Hit {
                    entity: EntityRef::new(EntityKind::Skill, skill.id().clone()),
                    heading: Some(skill.name().to_string()),
                    excerpt: skill.summary().to_string(),
                    namespace: skill.namespace().clone(),
                    updated_at: skill.updated_at(),
                    matches,
                });
            }
        }
        if !query.covers(EntityKind::Document) {
            return Ok(hits);
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

            let title_matches = document
                .title()
                .as_str()
                .to_lowercase()
                .matches(&needle)
                .count();
            if title_matches > 0 {
                hits.push(Hit {
                    entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                    heading: Some(document.title().to_string()),
                    excerpt: excerpt(document.body().as_str()),
                    namespace: document.namespace().clone(),
                    updated_at: document.updated_at(),
                    matches: title_matches,
                });
            }

            let sections = document.body().sections();
            if sections.is_empty() {
                let matches = document
                    .body()
                    .as_str()
                    .to_lowercase()
                    .matches(&needle)
                    .count();
                if matches > 0 || needle.is_empty() {
                    hits.push(Hit {
                        entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                        heading: None,
                        excerpt: excerpt(document.body().as_str()),
                        namespace: document.namespace().clone(),
                        updated_at: document.updated_at(),
                        matches,
                    });
                }
                continue;
            }

            for section in sections {
                let haystack = format!("{} {}", section.heading, section.body).to_lowercase();
                let matches = haystack.matches(&needle).count();
                if matches == 0 && !needle.is_empty() {
                    continue;
                }
                hits.push(Hit {
                    entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                    heading: Some(section.heading.clone()),
                    excerpt: excerpt(section.body),
                    namespace: document.namespace().clone(),
                    updated_at: document.updated_at(),
                    matches,
                });
            }
        }

        Ok(hits)
    }
}

fn excerpt(body: &str) -> String {
    body.trim().chars().take(240).collect()
}
