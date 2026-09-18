use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Id, TypeRegistry};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetDocumentFieldCommand {
    pub document_id: String,
    pub fields: Vec<(String, serde_json::Value)>,
}

pub struct SetDocumentField {
    documents: Arc<dyn DocumentStore>,
    types: Arc<dyn TypeRegistry>,
    clock: Arc<dyn Clock>,
}

impl SetDocumentField {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        types: Arc<dyn TypeRegistry>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            documents,
            types,
            clock,
        }
    }

    pub async fn execute(&self, cmd: SetDocumentFieldCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        for (field, value) in &cmd.fields {
            document.set_field(field, value.clone(), &*self.types, &*self.clock)?;
        }
        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
