use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    EntityKind, Hit, Result, Search, SearchQuery, SkillStore, document_passages, score,
    skill_passage,
};

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
        let mut passages = Vec::new();
        if query.covers(EntityKind::Skill) {
            for skill in self.skills.all().await? {
                if query.selects_skill(&skill) {
                    passages.push(skill_passage(&skill));
                }
            }
        }
        if query.covers(EntityKind::Document) {
            for document in self.documents.snapshot() {
                if query.selects_document(&document) {
                    passages.extend(document_passages(&document));
                }
            }
        }
        Ok(score(passages, &query.text))
    }
}
