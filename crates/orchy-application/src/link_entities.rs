use std::sync::Arc;

use orchy_core::{
    ActorStore, DocumentStatus, DocumentStore, DomainError, Edge, EdgeStore, EntityKind, EntityRef,
    MessageStore, Relation, SkillStore, TaskStore, UnitOfWork,
};
use serde::{Deserialize, Serialize};

use crate::dto::EdgeDto;
use crate::error::ApplicationResult;
use crate::unit_of_work::atomically;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinkEntitiesCommand {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub remove: bool,
}

/// Only a link being added must point at something real; removing one never does, so a
/// link left dangling by a deletion can always be taken away.
pub struct LinkEntities {
    edges: Arc<dyn EdgeStore>,
    documents: Arc<dyn DocumentStore>,
    tasks: Arc<dyn TaskStore>,
    skills: Arc<dyn SkillStore>,
    messages: Arc<dyn MessageStore>,
    actors: Arc<dyn ActorStore>,
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl LinkEntities {
    pub fn new(
        edges: Arc<dyn EdgeStore>,
        documents: Arc<dyn DocumentStore>,
        tasks: Arc<dyn TaskStore>,
        skills: Arc<dyn SkillStore>,
        messages: Arc<dyn MessageStore>,
        actors: Arc<dyn ActorStore>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            edges,
            documents,
            tasks,
            skills,
            messages,
            actors,
            unit_of_work,
        }
    }

    pub async fn execute(&self, cmd: LinkEntitiesCommand) -> ApplicationResult<EdgeDto> {
        atomically(&*self.unit_of_work, || self.apply(cmd.clone())).await
    }

    async fn apply(&self, cmd: LinkEntitiesCommand) -> ApplicationResult<EdgeDto> {
        let from: EntityRef = cmd.from.parse()?;
        let to: EntityRef = cmd.to.parse()?;
        let relation: Relation = cmd.relation.parse()?;

        if let Some(command) = relation.managed_by() {
            return Err(DomainError::forbidden(format!(
                "`{relation}` carries consequences beyond the edge; use `{command}`"
            ))
            .into());
        }
        let edge = Edge::new(from, to, relation)?;
        if cmd.remove {
            self.ensure_removable(&edge).await?;
            self.edges.remove(&edge).await?;
            return Ok(EdgeDto::from(&edge));
        }
        for end in [edge.from(), edge.to()] {
            self.ensure_exists(end).await?;
        }
        self.edges.add(&edge).await?;
        Ok(EdgeDto::from(&edge))
    }

    async fn ensure_removable(&self, edge: &Edge) -> ApplicationResult<()> {
        if edge.relation() != &Relation::DerivedFrom
            || edge.from().kind() != EntityKind::Skill
            || edge.to().kind() != EntityKind::Document
        {
            return Ok(());
        }
        let Some(candidate) = edge.to().id() else {
            return Ok(());
        };
        let promoted = self
            .documents
            .get(candidate)
            .await?
            .is_some_and(|d| d.is_candidate() && d.status() == Some(DocumentStatus::Promoted));
        if promoted {
            return Err(DomainError::forbidden(format!(
                "{} was promoted into {}; the link is the record of that promotion",
                edge.to(),
                edge.from()
            ))
            .into());
        }
        Ok(())
    }

    async fn ensure_exists(&self, entity: &EntityRef) -> ApplicationResult<()> {
        let exists = match (entity.kind(), entity.id(), entity.as_actor()) {
            (EntityKind::Document, Some(id), _) => self.documents.get(id).await?.is_some(),
            (EntityKind::Task, Some(id), _) => self.tasks.get(id).await?.is_some(),
            (EntityKind::Skill, Some(id), _) => self.skills.get(id).await?.is_some(),
            (EntityKind::Message, Some(id), _) => self.messages.get(id).await?.is_some(),
            (EntityKind::Actor, _, Some(actor)) => self.actors.get(actor).await?.is_some(),
            _ => false,
        };
        if exists {
            return Ok(());
        }
        Err(DomainError::not_found(entity.kind().as_str(), entity).into())
    }
}
