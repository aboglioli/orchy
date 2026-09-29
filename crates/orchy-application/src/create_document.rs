use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, Body, Clock, Document, DocumentStore, Edge, EdgeStore, EntityRef, Id,
    IdGenerator, Kind, Namespace, Relation, Tag, TaskStore, Title,
};
use serde::{Deserialize, Serialize};

use crate::dto::DocumentDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateDocumentCommand {
    pub actor: Option<String>,
    pub kind: String,
    pub title: String,
    pub namespace: Option<String>,
    pub body: Option<String>,
    pub tags: Vec<String>,
    pub produced_by: Option<String>,
}

pub struct CreateDocument {
    documents: Arc<dyn DocumentStore>,
    actors: Arc<dyn ActorStore>,
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
}

impl CreateDocument {
    pub fn new(
        documents: Arc<dyn DocumentStore>,
        actors: Arc<dyn ActorStore>,
        tasks: Arc<dyn TaskStore>,
        edges: Arc<dyn EdgeStore>,
        ids: Arc<dyn IdGenerator>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            documents,
            actors,
            tasks,
            edges,
            ids,
            clock,
        }
    }

    pub async fn execute(&self, cmd: CreateDocumentCommand) -> ApplicationResult<DocumentDto> {
        let kind = cmd.kind.parse::<Kind>()?;
        let producer = match &cmd.produced_by {
            Some(task) => Some(self.tasks.require(&Id::new(task)?).await?.id().clone()),
            None => None,
        };

        let namespace = match (&cmd.namespace, &cmd.actor) {
            (Some(ns), _) => Namespace::new(ns)?,
            (None, Some(actor)) => self.actors.home_of(&actor.parse::<ActorId>()?).await?,
            (None, None) => Namespace::root(),
        };

        let mut document = Document::create(
            kind,
            Title::new(&cmd.title)?,
            namespace,
            Body::new(cmd.body.unwrap_or_default()),
            &*self.ids,
            &*self.clock,
        );

        if !cmd.tags.is_empty() {
            let tags = cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?;
            document.retag(tags, &[], &*self.clock);
        }

        self.documents.save(&mut document).await?;
        if let Some(task) = producer {
            self.edges
                .add(&Edge::new(
                    EntityRef::task(task),
                    EntityRef::document(document.id().clone()),
                    Relation::Produces,
                )?)
                .await?;
        }
        Ok(DocumentDto::from(&document))
    }
}
