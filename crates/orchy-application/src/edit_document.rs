use std::sync::Arc;

use orchy_core::{Body, Clock, DocumentStore, Id};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::{ApplicationError, ApplicationResult};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EditDocumentCommand {
    pub document_id: String,
    pub content: String,
    pub mode: EditMode,
    pub if_match: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditMode {
    #[default]
    Append,
    Replace,
    Section(String),
    ReplaceIn(String),
}

pub struct EditDocument {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
}

impl EditDocument {
    pub fn new(documents: Arc<dyn DocumentStore>, clock: Arc<dyn Clock>) -> Self {
        Self { documents, clock }
    }

    pub async fn execute(&self, cmd: EditDocumentCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;

        if let Some(expected) = &cmd.if_match
            && document.content_hash() != expected
        {
            return Err(ApplicationError::Domain(orchy_core::DomainError::conflict(
                format!(
                    "document changed since it was read (expected {expected}, found {})",
                    document.content_hash()
                ),
            )));
        }

        match &cmd.mode {
            EditMode::Append => document.append(&cmd.content, &*self.clock),
            EditMode::Replace => document.edit(Body::new(&cmd.content), &*self.clock),
            EditMode::Section(heading) => {
                document.replace_section(heading, &cmd.content, &*self.clock)?
            }
            EditMode::ReplaceIn(needle) => {
                document.replace_once(needle, &cmd.content, &*self.clock)?
            }
        }

        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
