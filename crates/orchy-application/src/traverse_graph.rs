use std::sync::Arc;

use orchy_core::{EdgeStore, EntityRef, Relation};
use serde::{Deserialize, Serialize};

use crate::dto::EdgeDto;
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraverseGraphCommand {
    pub from: String,
    pub depth: Option<u8>,
    pub relations: Vec<String>,
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
        let relations = cmd
            .relations
            .iter()
            .map(|r| r.parse::<Relation>())
            .collect::<orchy_core::Result<Vec<_>>>()?;
        let hops = self
            .edges
            .neighbourhood(&from, cmd.depth.unwrap_or(1))
            .await?;
        if relations.is_empty() {
            return Ok(hops
                .iter()
                .map(|hop| TraversalHopDto {
                    edge: EdgeDto::from(&hop.edge),
                    depth: hop.depth,
                })
                .collect());
        }

        // only what is reachable through the chosen relations counts, however near
        let mut reached = vec![from];
        let mut kept = Vec::new();
        for hop in hops {
            if !relations.contains(hop.edge.relation()) {
                continue;
            }
            let (a, b) = (hop.edge.from(), hop.edge.to());
            let joins = reached.contains(a) || reached.contains(b);
            if !joins {
                continue;
            }
            for end in [a, b] {
                if !reached.contains(end) {
                    reached.push(end.clone());
                }
            }
            kept.push(TraversalHopDto {
                edge: EdgeDto::from(&hop.edge),
                depth: hop.depth,
            });
        }
        Ok(kept)
    }
}
