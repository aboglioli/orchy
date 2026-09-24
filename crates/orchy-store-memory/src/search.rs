use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{Hit, Passage, Result, Search, SearchQuery, document::score};

use crate::documents::MemoryDocumentStore;

const EXCERPT: usize = 240;

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
        let mut passages = Vec::new();

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
                document: document.id().clone(),
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
