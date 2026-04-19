use std::sync::Arc;

use orchy_core::edge::{EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{EdgeResponse, PageResponse};

pub struct ListEdgesCommand {
    pub org_id: String,
    pub rel_type: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
}

pub struct ListEdges {
    store: Arc<dyn EdgeStore>,
}

impl ListEdges {
    pub fn new(store: Arc<dyn EdgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: ListEdgesCommand) -> Result<PageResponse<EdgeResponse>> {
        let org =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let rel_type: Option<RelationType> = cmd
            .rel_type
            .map(|s| s.parse().map_err(Error::InvalidInput))
            .transpose()?;

        let page = PageParams::new(cmd.after, cmd.limit);
        let result = self
            .store
            .list_by_org(&org, rel_type.as_ref(), page, true)
            .await?;
        Ok(PageResponse::from(result))
    }
}
