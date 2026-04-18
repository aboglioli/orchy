use std::collections::HashSet;
use std::sync::Arc;

use orchy_core::edge::{EdgeStore, RelationType, TraversalDirection};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::{GraphResponse, TraversalEdgeResponse};

pub struct GetGraphCommand {
    pub org_id: String,
    pub kind: String,
    pub id: String,
    pub max_depth: Option<u32>,
    pub rel_types: Option<Vec<String>>,
    pub direction: Option<String>,
}

pub struct GetGraph {
    store: Arc<dyn EdgeStore>,
}

impl GetGraph {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: GetGraphCommand) -> Result<GraphResponse> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let kind = cmd
            .kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let max_depth = cmd.max_depth.unwrap_or(3).min(10);

        let rel_types: Option<Vec<RelationType>> = cmd
            .rel_types
            .map(|v| {
                v.into_iter()
                    .map(|s| s.parse().map_err(Error::InvalidInput))
                    .collect()
            })
            .transpose()?;

        let direction = match cmd.direction.as_deref() {
            Some("incoming") => TraversalDirection::Incoming,
            Some("both") => TraversalDirection::Both,
            _ => TraversalDirection::Outgoing,
        };

        let traversal = self
            .store
            .traverse(
                &org,
                &kind,
                &cmd.id,
                max_depth,
                rel_types.as_deref(),
                direction,
            )
            .await?;

        let mut node_ids: HashSet<String> = HashSet::new();
        node_ids.insert(format!("{}:{}", kind, cmd.id));
        for e in &traversal {
            node_ids.insert(format!("{}:{}", e.from_kind, e.from_id));
            node_ids.insert(format!("{}:{}", e.to_kind, e.to_id));
        }

        let edges: Vec<TraversalEdgeResponse> =
            traversal.iter().map(TraversalEdgeResponse::from).collect();
        let node_count = node_ids.len();

        Ok(GraphResponse {
            root_kind: kind.to_string(),
            root_id: cmd.id,
            edges,
            node_count,
        })
    }
}
