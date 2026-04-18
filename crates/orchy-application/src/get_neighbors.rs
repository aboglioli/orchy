use std::sync::Arc;

use orchy_core::edge::{EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::EdgeResponse;

pub struct GetNeighborsCommand {
    pub org_id: String,
    pub kind: String,
    pub id: String,
    /// None = both directions; "outgoing" | "incoming"
    pub direction: Option<String>,
    pub rel_type: Option<String>,
}

pub struct GetNeighbors {
    store: Arc<dyn EdgeStore>,
}

impl GetNeighbors {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: GetNeighborsCommand) -> Result<Vec<EdgeResponse>> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let kind = cmd
            .kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;
        let rel_type: Option<RelationType> = cmd
            .rel_type
            .map(|s| s.parse().map_err(Error::InvalidInput))
            .transpose()?;

        let edges = match cmd.direction.as_deref() {
            Some("incoming") => {
                self.store
                    .find_to(&org, &kind, &cmd.id, rel_type.as_ref())
                    .await?
            }
            Some("outgoing") => {
                self.store
                    .find_from(&org, &kind, &cmd.id, rel_type.as_ref())
                    .await?
            }
            _ => {
                let mut out = self
                    .store
                    .find_from(&org, &kind, &cmd.id, rel_type.as_ref())
                    .await?;
                let inc = self
                    .store
                    .find_to(&org, &kind, &cmd.id, rel_type.as_ref())
                    .await?;
                out.extend(inc);
                out
            }
        };

        Ok(edges.iter().map(EdgeResponse::from).collect())
    }
}
