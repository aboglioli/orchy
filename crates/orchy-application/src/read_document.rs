use std::sync::Arc;

use orchy_core::{DocumentStore, EdgeStore, EntityRef, Id};
use serde::{Deserialize, Serialize};

use crate::dto::{DocumentDto, EdgeDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReadDocumentCommand {
    pub document_id: String,
    pub section: Option<String>,
    pub nth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadDocumentResponse {
    pub document: DocumentDto,
    pub section: Option<String>,
    pub edges: Vec<EdgeDto>,
}

pub struct ReadDocument {
    documents: Arc<dyn DocumentStore>,
    edges: Arc<dyn EdgeStore>,
}

impl ReadDocument {
    pub fn new(documents: Arc<dyn DocumentStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { documents, edges }
    }

    pub async fn execute(
        &self,
        cmd: ReadDocumentCommand,
    ) -> ApplicationResult<ReadDocumentResponse> {
        let id = Id::new(&cmd.document_id)?;
        let document = self.documents.require(&id).await?;

        let section = match &cmd.section {
            Some(heading) => Some(document.body().section(heading, cmd.nth)?.body.to_owned()),
            None => None,
        };

        let edges = self.edges.out(&EntityRef::document(id), None).await?;

        Ok(ReadDocumentResponse {
            document: DocumentDto::from(&document),
            section,
            edges: edges.iter().map(EdgeDto::from).collect(),
        })
    }
}
