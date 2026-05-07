use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::graph::{EdgeId, EdgeStore};
use orchy_core::organization::OrganizationId;

pub struct RemoveEdgeCommand {
    pub edge_id: String,
    pub org_id: String,
}

pub struct RemoveEdge {
    store: Arc<dyn EdgeStore>,
}

impl RemoveEdge {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: RemoveEdgeCommand) -> ApplicationResult<()> {
        let id = cmd.edge_id.parse::<EdgeId>()?;
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let mut edge = self
            .store
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Edge,
                id: cmd.edge_id.to_string(),
            })?;

        if edge.org_id() != &org_id {
            return Err(Error::NotFound {
                resource: Resource::Edge,
                id: cmd.edge_id.to_string(),
            }
            .into());
        }

        edge.invalidate()?;
        self.store.save(&mut edge).await.map_err(Into::into)
    }
}
