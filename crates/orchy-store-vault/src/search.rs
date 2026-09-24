use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{Document, Hit, Passage, Result, Search, SearchQuery, document::score};

use crate::documents::VaultDocumentStore;

const EXCERPT: usize = 240;

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
        let mut passages = Vec::new();
        for document in self.documents.all().await? {
            if !selected(&document, query) {
                continue;
            }
            passages.extend(passages_of(&document));
        }
        Ok(score(passages, &query.text))
    }
}

fn selected(document: &Document, query: &SearchQuery) -> bool {
    if let Some(kinds) = &query.kind
        && !kinds.contains(document.kind())
    {
        return false;
    }
    if let Some(statuses) = &query.status {
        match document.status() {
            Some(status) if statuses.contains(&status) => {}
            _ => return false,
        }
    }
    if let Some(namespace) = &query.namespace
        && !namespace.contains(document.namespace())
    {
        return false;
    }
    query.tags.iter().all(|t| document.tags().contains(t))
}

fn passages_of(document: &Document) -> Vec<Passage> {
    let passage = |heading: Option<String>, body: &str| Passage {
        document: document.id().clone(),
        heading,
        title: document.title().to_string(),
        body: body.to_owned(),
        excerpt: body.trim().chars().take(EXCERPT).collect(),
        namespace: document.namespace().clone(),
        updated_at: document.updated_at(),
    };

    let sections = document.body().sections();
    if sections.is_empty() {
        return vec![passage(None, document.body().as_str())];
    }
    sections
        .iter()
        .map(|section| passage(Some(section.heading.clone()), section.body))
        .collect()
}
