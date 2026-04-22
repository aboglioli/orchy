use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeId, EdgeStore};

pub struct RemoveEdgeCommand {
    pub edge_id: String,
}

pub struct RemoveEdge {
    store: Arc<dyn EdgeStore>,
}

impl RemoveEdge {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: RemoveEdgeCommand) -> Result<()> {
        let id = cmd.edge_id.parse::<EdgeId>()?;
        let mut edge = self
            .store
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("edge: {}", cmd.edge_id)))?;
        edge.invalidate()?;
        self.store.save(&mut edge).await
    }
}
