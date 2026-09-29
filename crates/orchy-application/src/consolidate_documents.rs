use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, DomainError, Edge, EdgeStore, EntityRef, Id, Relation};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsolidateDocumentsCommand {
    pub sources: Vec<String>,
    pub into: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidateDocumentsResponse {
    pub into: DocumentDto,
    pub superseded: Vec<DocumentDto>,
}

/// Records a merge the agent already wrote into the target's body: the sources are
/// superseded by the target and their tags carried over.
pub struct ConsolidateDocuments {
    documents: Arc<dyn DocumentStore>,
    edges: Arc<dyn EdgeStore>,
    clock: Arc<dyn Clock>,
}

impl ConsolidateDocuments {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        edges: Arc<dyn EdgeStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            documents,
            edges,
            clock,
        }
    }

    pub async fn execute(
        &self,
        cmd: ConsolidateDocumentsCommand,
    ) -> ApplicationResult<ConsolidateDocumentsResponse> {
        let into_id = Id::new(&cmd.into)?;
        let mut source_ids = Vec::new();
        for raw in &cmd.sources {
            let id = Id::new(raw)?;
            if id == into_id {
                return Err(DomainError::validation(
                    "a document cannot be consolidated into itself",
                )
                .into());
            }
            if !source_ids.contains(&id) {
                source_ids.push(id);
            }
        }
        if source_ids.is_empty() {
            return Err(
                DomainError::validation("name at least one document to consolidate").into(),
            );
        }

        let mut into = self.documents.require(&into_id).await?;
        let mut superseded = Vec::new();
        for source_id in &source_ids {
            let mut source = self.documents.require(source_id).await?;
            source.supersede(into_id.clone(), &*self.clock)?;
            self.documents.save(&mut source).await?;
            into.retag(source.tags().to_vec(), &[], &*self.clock);
            superseded.push(DocumentDto::from(&source));
        }
        // links live in the target's file, so it is saved before they are added
        self.documents.save(&mut into).await?;

        for source_id in &source_ids {
            let (from, to) = (
                EntityRef::document(into_id.clone()),
                EntityRef::document(source_id.clone()),
            );
            self.edges
                .add(&Edge::new(from.clone(), to.clone(), Relation::Supersedes)?)
                .await?;
            self.edges
                .add(&Edge::new(from, to, Relation::MergedFrom)?)
                .await?;
        }

        Ok(ConsolidateDocumentsResponse {
            into: DocumentDto::from(&into),
            superseded,
        })
    }
}
