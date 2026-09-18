use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use orchy_core::{Document, DocumentQuery, DocumentStore, EventLog, Id, Page, PageRequest, Result};

use crate::eventlog::MemoryEventLog;

pub struct MemoryDocumentStore {
    documents: Mutex<BTreeMap<Id, Document>>,
    log: Arc<MemoryEventLog>,
}

impl MemoryDocumentStore {
    pub fn new(log: Arc<MemoryEventLog>) -> Self {
        Self {
            documents: Mutex::new(BTreeMap::new()),
            log,
        }
    }

    pub fn snapshot(&self) -> Vec<Document> {
        self.documents
            .lock()
            .expect("document mutex")
            .values()
            .cloned()
            .collect()
    }
}

#[async_trait]
impl DocumentStore for MemoryDocumentStore {
    async fn get(&self, id: &Id) -> Result<Option<Document>> {
        Ok(self
            .documents
            .lock()
            .expect("document mutex")
            .get(id)
            .cloned())
    }

    async fn find(&self, query: &DocumentQuery, page: PageRequest) -> Result<Page<Document>> {
        let mut matched: Vec<Document> = self
            .snapshot()
            .into_iter()
            .filter(|d| query.matches(d))
            .collect();
        matched.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(Page::slice(matched, page))
    }

    async fn save(&self, document: &mut Document) -> Result<()> {
        let events = document.drain_events();
        self.documents
            .lock()
            .expect("document mutex")
            .insert(document.id().clone(), document.clone());
        self.log.append(&events).await
    }

    async fn delete(&self, id: &Id) -> Result<()> {
        self.documents.lock().expect("document mutex").remove(id);
        Ok(())
    }
}
