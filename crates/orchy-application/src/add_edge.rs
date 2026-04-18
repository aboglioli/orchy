use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::EdgeResponse;

pub struct AddEdgeCommand {
    pub org_id: String,
    pub from_kind: String,
    pub from_id: String,
    pub to_kind: String,
    pub to_id: String,
    pub rel_type: String,
    pub display: Option<String>,
    pub created_by: Option<String>,
}

pub struct AddEdge {
    store: Arc<dyn EdgeStore>,
}

impl AddEdge {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: AddEdgeCommand) -> Result<EdgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let from_kind = cmd
            .from_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let to_kind = cmd
            .to_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let rel_type = cmd
            .rel_type
            .parse::<RelationType>()
            .map_err(Error::InvalidInput)?;
        let created_by = cmd
            .created_by
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let edge = Edge::new(
            org_id,
            from_kind,
            cmd.from_id,
            to_kind,
            cmd.to_id,
            rel_type,
            cmd.display,
            created_by,
        );
        self.store.save(&edge).await?;
        Ok(EdgeResponse::from(&edge))
    }
}
