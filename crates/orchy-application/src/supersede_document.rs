use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Edge, EdgeStore, EntityRef, Id, Relation, UnitOfWork};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SupersedeDocumentCommand {
    pub old_id: String,
    pub new_id: String,
    pub if_match: Option<String>,
}

pub struct SupersedeDocument {
    documents: Arc<dyn DocumentStore>,
    edges: Arc<dyn EdgeStore>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl SupersedeDocument {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        edges: Arc<dyn EdgeStore>,
        clock: Arc<dyn Clock>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            documents,
            edges,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: SupersedeDocumentCommand) -> ApplicationResult<DocumentDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: SupersedeDocumentCommand) -> ApplicationResult<DocumentDto> {
        let old_id = Id::new(&cmd.old_id)?;
        let new_id = Id::new(&cmd.new_id)?;
        let replacement = self.documents.require(&new_id).await?;

        let mut old = self.documents.require(&old_id).await?;
        old.ensure_unchanged(cmd.if_match.as_deref())?;
        old.supersede(&replacement, &*self.clock)?;
        self.documents.save(&mut old).await?;

        self.edges
            .add(&Edge::new(
                EntityRef::document(new_id),
                EntityRef::document(old_id),
                Relation::Supersedes,
            )?)
            .await?;

        Ok(DocumentDto::from(&old))
    }
}
