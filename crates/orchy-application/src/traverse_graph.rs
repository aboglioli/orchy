use std::sync::Arc;

use orchy_core::{DocumentStore, EdgeStore, EntityKind, EntityRef, Relation};
use serde::{Deserialize, Serialize};

use crate::dto::EdgeDto;
use crate::error::ApplicationResult;
use crate::resolve_mentions::ResolveMentions;

const MENTIONS: &str = "mentions";

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
    documents: Arc<dyn DocumentStore>,
    mentions: Arc<ResolveMentions>,
}

impl TraverseGraph {
    pub fn new(
        edges: Arc<dyn EdgeStore>,
        documents: Arc<dyn DocumentStore>,
        mentions: Arc<ResolveMentions>,
    ) -> Self {
        Self {
            edges,
            documents,
            mentions,
        }
    }

    pub async fn execute(
        &self,
        cmd: TraverseGraphCommand,
    ) -> ApplicationResult<Vec<TraversalHopDto>> {
        let from: EntityRef = cmd.from.parse()?;
        let depth = cmd.depth.unwrap_or(1);
        let wants_mentions =
            cmd.relations.is_empty() || cmd.relations.iter().any(|r| r == MENTIONS);
        let relations = cmd
            .relations
            .iter()
            .filter(|r| *r != MENTIONS)
            .map(|r| r.parse::<Relation>())
            .collect::<orchy_core::Result<Vec<_>>>()?;
        let mut kept = self
            .linked(&from, depth, &relations, cmd.relations.is_empty())
            .await?;
        if wants_mentions {
            let mentioned = self.mentioned(&from, depth, &kept).await?;
            kept.extend(mentioned);
        }
        Ok(kept)
    }

    async fn linked(
        &self,
        from: &EntityRef,
        depth: u8,
        relations: &[Relation],
        everything: bool,
    ) -> ApplicationResult<Vec<TraversalHopDto>> {
        let hops = self.edges.neighbourhood(from, depth).await?;
        if everything {
            return Ok(hops
                .iter()
                .map(|hop| TraversalHopDto {
                    edge: EdgeDto::from(&hop.edge),
                    depth: hop.depth,
                })
                .collect());
        }

        // only what is reachable through the chosen relations counts, however near
        let mut reached = vec![from.clone()];
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

    async fn mentioned(
        &self,
        from: &EntityRef,
        depth: u8,
        hops: &[TraversalHopDto],
    ) -> ApplicationResult<Vec<TraversalHopDto>> {
        let mut nodes = vec![(from.to_string(), 0)];
        for hop in hops {
            for end in [&hop.edge.from, &hop.edge.to] {
                if !nodes.iter().any(|(n, _)| n == end) {
                    nodes.push((end.clone(), hop.depth));
                }
            }
        }

        let mut found = Vec::new();
        for (node, at) in nodes {
            if at >= depth {
                continue;
            }
            let Ok(entity) = node.parse::<EntityRef>() else {
                continue;
            };
            if entity.kind() != EntityKind::Document {
                continue;
            }
            let Some(document) = self.documents.get(entity.id()).await? else {
                continue;
            };
            for target in self.mentions.execute(&document).await? {
                found.push(TraversalHopDto {
                    edge: EdgeDto {
                        from: node.clone(),
                        to: target.to_string(),
                        relation: MENTIONS.to_owned(),
                    },
                    depth: at + 1,
                });
            }
        }
        Ok(found)
    }
}
