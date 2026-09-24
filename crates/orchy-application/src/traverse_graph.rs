use std::sync::Arc;

use orchy_core::{EdgeStore, EntityRef};
use serde::{Deserialize, Serialize};

use crate::dto::EdgeDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraverseGraphCommand {
    pub from: String,
    pub depth: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraversalHopDto {
    pub edge: EdgeDto,
    pub depth: u8,
}

pub struct TraverseGraph {
    edges: Arc<dyn EdgeStore>,
}

impl TraverseGraph {
    pub fn new(edges: Arc<dyn EdgeStore>) -> Self {
        Self { edges }
    }

    pub async fn execute(
        &self,
        cmd: TraverseGraphCommand,
    ) -> ApplicationResult<Vec<TraversalHopDto>> {
        let from: EntityRef = cmd.from.parse()?;
        let hops = self
            .edges
            .neighbourhood(&from, cmd.depth.unwrap_or(1))
            .await?;
        Ok(hops
            .iter()
            .map(|hop| TraversalHopDto {
                edge: EdgeDto::from(&hop.edge),
                depth: hop.depth,
            })
            .collect())
    }
}
