use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Id, Kind, Namespace};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromoteDocumentCommand {
    pub document_id: String,
    pub into: String,
    pub namespace: Option<String>,
}

pub struct PromoteDocument {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
}

impl PromoteDocument {
    pub fn new(documents: Arc<dyn DocumentStore>, clock: Arc<dyn Clock>) -> Self {
        Self { documents, clock }
    }

    pub async fn execute(&self, cmd: PromoteDocumentCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        let into = match &cmd.namespace {
            Some(ns) => Namespace::new(ns)?,
            None => Namespace::root(),
        };
        document.promote(cmd.into.parse::<Kind>()?, into, &*self.clock)?;
        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
