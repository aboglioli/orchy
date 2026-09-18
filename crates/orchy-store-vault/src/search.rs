use std::sync::Arc;

use async_trait::async_trait;
use grep_matcher::Matcher;
use grep_regex::RegexMatcher;
use grep_searcher::{Searcher, sinks::UTF8};
use orchy_core::{DomainError, Hit, Result, Search, SearchQuery};

use crate::documents::VaultDocumentStore;

pub struct VaultSearch {
    documents: Arc<VaultDocumentStore>,
}

impl VaultSearch {
    pub fn new(documents: Arc<VaultDocumentStore>) -> Self {
        Self { documents }
    }
}

#[async_trait]
impl Search for VaultSearch {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>> {
        let matcher = RegexMatcher::new_line_matcher(&regex_syntax::escape(&query.text))
            .map_err(|e| DomainError::validation(format!("bad search pattern: {e}")))?;

        let mut hits = Vec::new();
        for document in self.documents.all().await? {
            if let Some(kinds) = &query.kind
                && !kinds.contains(document.kind())
            {
                continue;
            }
            if let Some(statuses) = &query.status {
                match document.status() {
                    Some(status) if statuses.contains(status) => {}
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

            for section in document.body().sections() {
                let haystack = format!("{}\n{}", section.heading, section.body);
                let matches = count_matches(&matcher, &haystack)?;
                if matches == 0 && !query.text.is_empty() {
                    continue;
                }
                hits.push(Hit {
                    document: document.id().clone(),
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
                        document: document.id().clone(),
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
