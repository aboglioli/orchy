use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Id, UnitOfWork};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetDocumentFieldCommand {
    pub document_id: String,
    pub fields: Vec<(String, Value)>,
    pub if_match: Option<String>,
}

pub struct SetDocumentField {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl SetDocumentField {
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

    pub async fn execute(&self, cmd: SetDocumentFieldCommand) -> ApplicationResult<DocumentDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: SetDocumentFieldCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;
        document.ensure_unchanged(cmd.if_match.as_deref())?;
        for (field, value) in &cmd.fields {
            document.set_field(field, value.clone(), &*self.clock)?;
        }
        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
