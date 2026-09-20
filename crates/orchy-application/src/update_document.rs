use std::sync::Arc;

use orchy_core::{Clock, DocumentStatus, DocumentStore, Id, Kind, Namespace, Tag, Title};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UpdateDocumentCommand {
    pub document_id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub namespace: Option<String>,
    pub status: Option<String>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
}

pub struct UpdateDocument {
    documents: Arc<dyn DocumentStore>,
    clock: Arc<dyn Clock>,
}

impl UpdateDocument {
    pub fn new(documents: Arc<dyn DocumentStore>, clock: Arc<dyn Clock>) -> Self {
        Self { documents, clock }
    }

    pub async fn execute(&self, cmd: UpdateDocumentCommand) -> ApplicationResult<DocumentDto> {
        let mut document = self.documents.require(&Id::new(&cmd.document_id)?).await?;

        if let Some(title) = &cmd.title {
            document.retitle(Title::new(title)?, &*self.clock);
        }
        if let Some(kind) = &cmd.kind {
            document.retype(kind.parse::<Kind>()?, &*self.clock)?;
        }
        if let Some(namespace) = &cmd.namespace {
            document.move_to(Namespace::new(namespace)?, &*self.clock);
        }
        if let Some(status) = &cmd.status {
            document.set_status(status.parse::<DocumentStatus>()?, &*self.clock)?;
        }
        if !cmd.add_tags.is_empty() || !cmd.remove_tags.is_empty() {
            let add = cmd
                .add_tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            let remove = cmd
                .remove_tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            document.retag(add, &remove, &*self.clock);
        }

        self.documents.save(&mut document).await?;
        Ok(DocumentDto::from(&document))
    }
}
