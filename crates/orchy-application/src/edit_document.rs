use std::sync::Arc;

use orchy_core::{Body, Clock, DocumentStore, Id};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

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
    Section {
        heading: String,
        nth: Option<usize>,
    },
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

        document.ensure_unchanged(cmd.if_match.as_deref())?;

        match &cmd.mode {
            EditMode::Append => document.append(&cmd.content, &*self.clock),
            EditMode::Replace => document.edit(Body::new(&cmd.content), &*self.clock),
            EditMode::Section { heading, nth } => {
                document.replace_section(heading, *nth, &cmd.content, &*self.clock)?
            }
            EditMode::ReplaceIn(needle) => {
                document.replace_once(needle, &cmd.content, &*self.clock)?
            }
        }

        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
