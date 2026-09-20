use std::sync::Arc;

use orchy_core::{DomainError, Edge, EdgeStore, EntityRef, Relation};
use serde::{Deserialize, Serialize};

use crate::dto::EdgeDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LinkEntitiesCommand {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub remove: bool,
}

pub struct LinkEntities {
    edges: Arc<dyn EdgeStore>,
}

impl LinkEntities {
    pub fn new(edges: Arc<dyn EdgeStore>) -> Self {
        Self { edges }
    }

    pub async fn execute(&self, cmd: LinkEntitiesCommand) -> ApplicationResult<EdgeDto> {
        let from: EntityRef = cmd.from.parse()?;
        let to: EntityRef = cmd.to.parse()?;
        let relation: Relation = cmd.relation.parse()?;

        if let Some(command) = relation.managed_by() {
            return Err(DomainError::forbidden(format!(
                "`{relation}` carries consequences beyond the edge; use `{command}`"
            ))
            .into());
        }
        if from == to && !relation.is_symmetric() {
            return Err(DomainError::validation(format!(
                "`{relation}` cannot point an entity at itself"
            ))
            .into());
        }
        relation.validate(from.kind(), to.kind())?;

        let edge = Edge::new(from, to, relation);
        if cmd.remove {
            self.edges.remove(&edge).await?;
        } else {
            self.edges.add(&edge).await?;
        }
        Ok(EdgeDto::from(&edge))
    }
}
