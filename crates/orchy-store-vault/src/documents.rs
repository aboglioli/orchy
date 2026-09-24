use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{
    Document, DocumentQuery, DocumentStore, EntityKind, EventLog, Id, Page, PageRequest, Result,
};

use crate::codec;
use crate::vault::{Precondition, Vault};

pub struct VaultDocumentStore {
    vault: Arc<Vault>,
    log: Arc<dyn EventLog>,
}

impl VaultDocumentStore {
    pub fn new(vault: Arc<Vault>, log: Arc<dyn EventLog>) -> Self {
        Self { vault, log }
    }

    pub async fn all(&self) -> Result<Vec<Document>> {
        let mut documents = Vec::new();
        for (key, file) in self.vault.load_all(EntityKind::Document).await? {
            documents.push(codec::document_from_markdown(&file, &key)?);
        }
        documents.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(documents)
    }
}

fn parent_of(key: &str) -> String {
    match key.rsplit_once('/') {
        Some((folder, _)) => format!("{folder}/"),
        None => String::new(),
    }
}

#[async_trait]
impl DocumentStore for VaultDocumentStore {
    async fn get(&self, id: &Id) -> Result<Option<Document>> {
        let Some((key, file)) = self.vault.read_by_id(id).await? else {
            return Ok(None);
        };
        if matches!(
            codec::kind_of(&file),
            Some("task") | Some("message") | Some("agent")
        ) {
            return Ok(None);
        }
        codec::document_from_markdown(&file, &key).map(Some)
    }

    async fn find(&self, query: &DocumentQuery, page: PageRequest) -> Result<Page<Document>> {
        let matched: Vec<Document> = self
            .all()
            .await?
            .into_iter()
            .filter(|d| query.matches(d))
            .collect();
        Ok(Page::slice(matched, page))
    }

    async fn save(&self, document: &mut Document) -> Result<()> {
        let events = document.drain_events();
        let placed = self
            .vault
            .layout()
            .document_key(document.namespace(), document.id());
        let key = match self.vault.locate(document.id()) {
            // a document someone filed by hand stays where they put it, as long as it is still
            // inside the namespace it claims
            Some(located) if located.key.starts_with(&parent_of(&placed)) => located.key,
            _ => placed,
        };

        let file = codec::document_to_markdown(document);
        self.vault
            .write_if(
                &key,
                &file,
                document.id(),
                EntityKind::Document,
                Precondition::Unchanged,
            )
            .await?;
        self.log.append(&events).await
    }

    async fn delete(&self, id: &Id) -> Result<()> {
        self.vault.remove(id).await
    }
}
