use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    Document, EntityKind, EntityRef, Hit, Passage, Result, Search, SearchQuery, Skill, SkillStore,
    score,
};

use crate::documents::VaultDocumentStore;
use crate::skills::VaultSkillStore;

const EXCERPT: usize = 240;

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
        let mut passages = Vec::new();
        if query.covers(EntityKind::Skill) {
            for skill in self.skills.all().await? {
                if skill_selected(&skill, query) {
                    passages.push(skill_passage(&skill));
                }
            }
        }
        if query.covers(EntityKind::Document) {
            for document in self.documents.all().await? {
                if document_selected(&document, query) {
                    passages.extend(document_passages(&document));
                }
            }
        }
        Ok(score(passages, &query.text))
    }
}

fn skill_selected(skill: &Skill, query: &SearchQuery) -> bool {
    if !query.retired && !skill.is_active() {
        return false;
    }
    if let Some(namespace) = &query.namespace
        && !namespace.contains(skill.namespace())
    {
        return false;
    }
    query.tags.iter().all(|t| skill.tags().contains(t))
}

fn skill_passage(skill: &Skill) -> Passage {
    Passage {
        entity: EntityRef::new(EntityKind::Skill, skill.id().clone()),
        heading: Some(skill.name().to_string()),
        title: format!("{} {}", skill.name(), skill.summary()),
        body: skill.body().as_str().to_owned(),
        excerpt: skill.summary().to_string(),
        namespace: skill.namespace().clone(),
        updated_at: skill.updated_at(),
    }
}

fn document_selected(document: &Document, query: &SearchQuery) -> bool {
    if let Some(kinds) = &query.kind
        && !kinds.contains(document.kind())
    {
        return false;
    }
    if !query.admits(document.status()) {
        return false;
    }
    if let Some(namespace) = &query.namespace
        && !namespace.contains(document.namespace())
    {
        return false;
    }
    query.tags.iter().all(|t| document.tags().contains(t))
}

fn document_passages(document: &Document) -> Vec<Passage> {
    let passage = |heading: Option<String>, body: &str| Passage {
        entity: EntityRef::new(EntityKind::Document, document.id().clone()),
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
