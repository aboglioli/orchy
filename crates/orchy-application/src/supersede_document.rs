use std::sync::Arc;

use orchy_core::{Clock, DocumentStore, Edge, EdgeStore, EntityRef, Id, Relation};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SupersedeDocumentCommand {
    pub old_id: String,
    pub new_id: String,
}

pub struct SupersedeDocument {
    documents: Arc<dyn DocumentStore>,
    edges: Arc<dyn EdgeStore>,
    clock: Arc<dyn Clock>,
}

impl SupersedeDocument {
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

    pub async fn execute(&self, cmd: SupersedeDocumentCommand) -> ApplicationResult<DocumentDto> {
        let old_id = Id::new(&cmd.old_id)?;
        let new_id = Id::new(&cmd.new_id)?;
        self.documents.require(&new_id).await?;

        let mut old = self.documents.require(&old_id).await?;
        old.supersede(new_id.clone(), &*self.clock)?;
        self.documents.save(&mut old).await?;

        self.edges
            .add(&Edge::new(
                EntityRef::document(old_id),
                EntityRef::document(new_id),
                Relation::Supersedes,
            ))
            .await?;

        Ok(DocumentDto::from(&old))
    }
}
