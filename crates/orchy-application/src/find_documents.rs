use std::sync::Arc;

use orchy_core::{DocumentQuery, DocumentStore, Kind, Namespace, PageRequest, Status, Tag};
use serde::{Deserialize, Serialize};

use crate::dto::{DocumentDto, PageDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FindDocumentsCommand {
    pub kind: Vec<String>,
    pub status: Vec<String>,
    pub namespace: Option<String>,
    pub tags: Vec<String>,
    pub text: Option<String>,
    pub offset: Option<usize>,
    pub limit: Option<usize>,
}

pub struct FindDocuments {
    documents: Arc<dyn DocumentStore>,
}

impl FindDocuments {
    pub fn new(documents: Arc<dyn DocumentStore>) -> Self {
        Self { documents }
    }

    pub async fn execute(
        &self,
        cmd: FindDocumentsCommand,
    ) -> ApplicationResult<PageDto<DocumentDto>> {
        let query = DocumentQuery {
            kind: if cmd.kind.is_empty() {
                None
            } else {
                Some(
                    cmd.kind
                        .iter()
                        .map(Kind::new)
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            status: if cmd.status.is_empty() {
                None
            } else {
                Some(
                    cmd.status
                        .iter()
                        .map(Status::new)
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
            tags: cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?,
            text: cmd.text,
            updated_since: None,
        };
        let page = PageRequest::new(
            cmd.offset.unwrap_or(0),
            cmd.limit.unwrap_or(PageRequest::default().limit()),
        );
        let found = self.documents.find(&query, page).await?;
        Ok(PageDto::new(
            found.items.iter().map(DocumentDto::from).collect(),
            found.total,
            found.offset,
            found.limit,
        ))
    }
}
