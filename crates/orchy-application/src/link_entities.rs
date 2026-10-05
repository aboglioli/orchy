use std::sync::Arc;

use orchy_core::{
    DocumentStore, DomainError, Edge, EdgeStore, EntityKind, EntityRef, MessageStore, Relation,
    SkillStore, TaskStore, UnitOfWork,
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
    unit_of_work: Arc<dyn UnitOfWork>,
}

impl LinkEntities {
    pub fn new(
        edges: Arc<dyn EdgeStore>,
        documents: Arc<dyn DocumentStore>,
        tasks: Arc<dyn TaskStore>,
        skills: Arc<dyn SkillStore>,
        messages: Arc<dyn MessageStore>,
        unit_of_work: Arc<dyn UnitOfWork>,
    ) -> Self {
        Self {
            edges,
            documents,
            tasks,
            skills,
            messages,
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
            self.edges.remove(&edge).await?;
            return Ok(EdgeDto::from(&edge));
        }
        for end in [edge.from(), edge.to()] {
            self.ensure_exists(end).await?;
        }
        self.edges.add(&edge).await?;
        Ok(EdgeDto::from(&edge))
    }

    async fn ensure_exists(&self, entity: &EntityRef) -> ApplicationResult<()> {
        let id = entity.id();
        let exists = match entity.kind() {
            EntityKind::Document => self.documents.get(id).await?.is_some(),
            EntityKind::Task => self.tasks.get(id).await?.is_some(),
            EntityKind::Skill => self.skills.get(id).await?.is_some(),
            EntityKind::Message => self.messages.get(id).await?.is_some(),
            EntityKind::Actor => true,
        };
        if exists {
            return Ok(());
        }
        Err(DomainError::not_found(entity.kind().as_str(), id.to_string()).into())
    }
}
