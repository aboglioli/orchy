use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Id, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RejectDocumentCommand {
    pub document_id: String,
    pub reason: Option<String>,
    pub if_match: Option<String>,
}

pub struct RejectDocument {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl RejectDocument {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            documents,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: RejectDocumentCommand) -> ApplicationResult<DocumentDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: RejectDocumentCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        document.ensure_unchanged(cmd.if_match.as_deref())?;
        document.reject(cmd.reason, &*self.clock)?;
        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
