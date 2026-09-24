use std::sync::Arc;

use async_trait::async_trait;
use grep_matcher::Matcher;
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{Searcher, sinks::UTF8};
use orchy_core::{
    DomainError, EntityKind, EntityRef, Hit, Result, Search, SearchQuery, SkillStore,
};

use crate::documents::VaultDocumentStore;
use crate::skills::VaultSkillStore;

pub struct VaultSearch {
    documents: Arc<VaultDocumentStore>,
    skills: Arc<VaultSkillStore>,
}

impl VaultSearch {
    pub fn new(documents: Arc<VaultDocumentStore>, skills: Arc<VaultSkillStore>) -> Self {
        Self { documents, skills }
    }
}

#[async_trait]
impl Search for VaultSearch {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>> {
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(true)
            .build(&regex_syntax::escape(&query.text))
            .map_err(|e| DomainError::validation(format!("bad search pattern: {e}")))?;

        let mut hits = Vec::new();
        if query.covers(EntityKind::Skill) {
            hits.extend(self.skills(query, &matcher).await?);
        }
        if !query.covers(EntityKind::Document) {
            return Ok(hits);
        }

        for document in self.documents.all().await? {
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

            let title_matches = count_matches(&matcher, document.title().as_str())?;
            if title_matches > 0 {
                hits.push(Hit {
                    entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                    heading: Some(document.title().to_string()),
                    excerpt: document.body().as_str().trim().chars().take(240).collect(),
                    namespace: document.namespace().clone(),
                    updated_at: document.updated_at(),
                    matches: title_matches,
                });
            }

            for section in document.body().sections() {
                let haystack = format!("{}\n{}", section.heading, section.body);
                let matches = count_matches(&matcher, &haystack)?;
                if matches == 0 && !query.text.is_empty() {
                    continue;
                }
                hits.push(Hit {
                    entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                    heading: Some(section.heading.clone()),
                    excerpt: section.body.trim().chars().take(240).collect(),
                    namespace: document.namespace().clone(),
                    updated_at: document.updated_at(),
                    matches,
                });
            }

            if document.body().sections().is_empty() {
                let matches = count_matches(&matcher, document.body().as_str())?;
                if matches > 0 || query.text.is_empty() {
                    hits.push(Hit {
                        entity: EntityRef::new(EntityKind::Document, document.id().clone()),
                        heading: None,
                        excerpt: document.body().as_str().trim().chars().take(240).collect(),
                        namespace: document.namespace().clone(),
                        updated_at: document.updated_at(),
                        matches,
                    });
                }
            }
        }
        Ok(hits)
    }
}

impl VaultSearch {
    async fn skills(&self, query: &SearchQuery, matcher: &RegexMatcher) -> Result<Vec<Hit>> {
        let mut hits = Vec::new();
        for skill in self.skills.all().await? {
            if !query.retired && !skill.is_active() {
                continue;
            }
            if let Some(namespace) = &query.namespace
                && !namespace.contains(skill.namespace())
            {
                continue;
            }
            if !query.tags.iter().all(|t| skill.tags().contains(t)) {
                continue;
            }

            let headline = format!("{} {}", skill.name(), skill.summary());
            let matches =
                count_matches(matcher, &headline)? + count_matches(matcher, skill.body().as_str())?;
            if matches == 0 && !query.text.is_empty() {
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
        Ok(hits)
    }
}

fn count_matches(matcher: &RegexMatcher, haystack: &str) -> Result<usize> {
    let mut count = 0;
    Searcher::new()
        .search_slice(
            matcher,
            haystack.as_bytes(),
            UTF8(|_, line| {
                count += matcher.find_iter(line.as_bytes(), |_| true).is_ok() as usize;
                Ok(true)
            }),
        )
        .map_err(|e| DomainError::validation(format!("search failed: {e}")))?;
    Ok(count)
}
