use std::sync::Arc;

use async_trait::async_trait;
use orchy_core::{Document, DocumentQuery, DocumentStore, EntityKind, EventLog, Id, Result};

use crate::codec;
use crate::placement;
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
        for (_, file) in self.vault.load_all(EntityKind::Document).await? {
            if let Ok(decoded) = codec::document_from_markdown(&file) {
                documents.push(decoded);
            }
        }
        documents.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(documents)
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
            Some("task") | Some("message") | Some("agent") | Some("skill")
        ) {
            return Ok(None);
        }
        codec::document_from_markdown(&file)
            .map_err(codec::at(&key))
            .map(Some)
    }

    async fn matching(&self, query: &DocumentQuery) -> Result<Vec<Document>> {
        Ok(self
            .all()
            .await?
            .into_iter()
            .filter(|d| query.matches(d))
            .collect())
    }

    async fn save(&self, document: &mut Document) -> Result<()> {
        let events = document.drain_events();
        let key = placement::of_document(&self.vault, document).await?;

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
}
