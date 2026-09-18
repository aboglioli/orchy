use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{Hit, Result, Search, SearchQuery};

use crate::documents::MemoryDocumentStore;

pub struct MemorySearch {
    documents: Arc<MemoryDocumentStore>,
}

impl MemorySearch {
    pub fn new(documents: Arc<MemoryDocumentStore>) -> Self {
        Self { documents }
    }
}

#[async_trait]
impl Search for MemorySearch {
    async fn sections(&self, query: &SearchQuery) -> Result<Vec<Hit>> {
        let needle = query.text.to_lowercase();
        let mut hits = Vec::new();

        for document in self.documents.snapshot() {
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
                        document: document.id().clone(),
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
                    document: document.id().clone(),
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
