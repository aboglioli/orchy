use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Id};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RejectDocumentCommand {
    pub document_id: String,
    pub reason: Option<String>,
    pub if_match: Option<String>,
}

pub struct RejectDocument {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
}

impl RejectDocument {
    pub fn new(documents: Arc<dyn DocumentStore>, clock: Arc<dyn Clock>) -> Self {
        Self { documents, clock }
    }

    pub async fn execute(&self, cmd: RejectDocumentCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        document.ensure_unchanged(cmd.if_match.as_deref())?;
        document.reject(cmd.reason, &*self.clock)?;
        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
