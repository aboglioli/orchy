use std::sync::Arc;

use orchy_core::{
    ActorId, ActorStore, Body, Clock, Document, DocumentStatus, DocumentStore, Edge, EdgeStore,
    EntityKind, EntityRef, Hit, Id, IdGenerator, Kind, Namespace, Relation, Search, SearchQuery,
    Tag, TaskStore, Title, UnitOfWork,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::dto::{DocumentDto, HitDto};
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreateDocumentCommand {
    pub actor: Option<String>,
    pub kind: String,
    pub title: String,
    pub namespace: Option<String>,
    pub body: Option<String>,
    pub tags: Vec<String>,
    pub produced_by: Option<String>,
    pub fields: Vec<(String, Value)>,
}

const MAX_SIMILAR: usize = 3;
/// Another document counts as similar when it matches the new title at least half as well
/// as the new document itself does, which keeps the bar independent of vault size.
const SIMILAR_SHARE: f64 = 0.5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDocumentResponse {
    #[serde(flatten)]
    pub document: DocumentDto,
    pub similar: Vec<HitDto>,
}

pub struct CreateDocumentSources {
    pub documents: Arc<dyn DocumentStore>,
    pub search: Arc<dyn Search>,
    pub actors: Arc<dyn ActorStore>,
    pub tasks: Arc<dyn TaskStore>,
    pub edges: Arc<dyn EdgeStore>,
    pub ids: Arc<dyn IdGenerator>,
    pub clock: Arc<dyn Clock>,
    pub unit_of_work: Arc<dyn UnitOfWork>,
}

pub struct CreateDocument {
    documents: Arc<dyn DocumentStore>,
    search: Arc<dyn Search>,
    actors: Arc<dyn ActorStore>,
    tasks: Arc<dyn TaskStore>,
    edges: Arc<dyn EdgeStore>,
    ids: Arc<dyn IdGenerator>,
    clock: Arc<dyn Clock>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl CreateDocument {
    pub fn new(sources: CreateDocumentSources) -> Self {
        let CreateDocumentSources {
            documents,
            search,
            actors,
            tasks,
            edges,
            ids,
            clock,
            unit_of_work,
        } = sources;
        Self {
            documents,
            search,
            actors,
            tasks,
            edges,
            ids,
            clock,
            unit_of_work,
        }
    }

    pub async fn execute(
        &self,
        cmd: CreateDocumentCommand,
    ) -> ApplicationResult<CreateDocumentResponse> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: CreateDocumentCommand) -> ApplicationResult<CreateDocumentResponse> {
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

        for (field, value) in cmd.fields {
            document.set_field(&field, value, &*self.clock)?;
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
        Ok(CreateDocumentResponse {
            similar: self.similar_to(&document).await?,
            document: DocumentDto::from(&document),
        })
    }

    async fn similar_to(&self, document: &Document) -> ApplicationResult<Vec<HitDto>> {
        let hits = self
            .search
            .sections(&SearchQuery {
                text: document.title().to_string(),
                entities: Some(vec![EntityKind::Document]),
                exclude_status: DocumentStatus::RETIRED.to_vec(),
                limit: usize::MAX,
                ..Default::default()
            })
            .await?;

        let own = hits
            .iter()
            .filter(|h| h.entity.id() == document.id())
            .map(|h| h.relevance)
            .fold(0.0, f64::max);
        if own <= 0.0 {
            return Ok(Vec::new());
        }

        let mut best: Vec<&Hit> = Vec::new();
        for hit in hits.iter().filter(|h| h.entity.id() != document.id()) {
            if hit.relevance < own * SIMILAR_SHARE {
                continue;
            }
            match best.iter_mut().find(|b| b.entity == hit.entity) {
                Some(kept) if kept.relevance < hit.relevance => *kept = hit,
                Some(_) => {}
                None => best.push(hit),
            }
        }
        best.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
        Ok(best
            .into_iter()
            .take(MAX_SIMILAR)
            .map(HitDto::from)
            .collect())
    }
}
