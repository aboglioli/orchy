use std::sync::Arc;

use orchy_core::{Edge, EdgeStore, EntityRef, RelationRegistry, RelationType};
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
    relations: Arc<dyn RelationRegistry>,
}

impl LinkEntities {
    pub fn new(edges: Arc<dyn EdgeStore>, relations: Arc<dyn RelationRegistry>) -> Self {
        Self { edges, relations }
    }

    pub async fn execute(&self, cmd: LinkEntitiesCommand) -> ApplicationResult<EdgeDto> {
        let from: EntityRef = cmd.from.parse()?;
        let to: EntityRef = cmd.to.parse()?;
        let relation = RelationType::new(&cmd.relation)?;

        self.relations.validate(&relation, from.kind(), to.kind())?;

        let edge = Edge::new(from, to, relation);
        if cmd.remove {
            self.edges.remove(&edge).await?;
        } else {
            self.edges.add(&edge).await?;
        }
        Ok(EdgeDto::from(&edge))
    }
}
