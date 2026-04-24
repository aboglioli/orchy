use std::sync::Arc;

use chrono::{DateTime, Utc};

use orchy_core::error::{Error, Result};
use orchy_core::graph::{EdgeStore, RelationType};
use orchy_core::organization::OrganizationId;
use orchy_core::pagination::PageParams;

use crate::dto::{EdgeDto, PageResponse};

pub struct ListEdgesCommand {
    pub org_id: String,
    pub rel_type: Option<String>,
    pub after: Option<String>,
    pub limit: Option<u32>,
    pub as_of: Option<DateTime<Utc>>,
}

pub struct ListEdges {
    edges: Arc<dyn EdgeStore>,
}

impl ListEdges {
    pub fn new(edges: Arc<dyn EdgeStore>) -> Self {
        Self { edges }
    }

    pub async fn execute(&self, cmd: ListEdgesCommand) -> Result<PageResponse<EdgeDto>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let rel_type = cmd
            .rel_type
            .as_deref()
            .map(|s| s.parse::<RelationType>())
            .transpose()
            .map_err(|e| Error::InvalidInput(e))?;

        let page = self
            .edges
            .list_by_org(
                &org_id,
                rel_type.as_ref(),
                PageParams::new(cmd.after, cmd.limit),
                true,
                cmd.as_of,
            )
            .await?;

        Ok(PageResponse::from(page))
    }
}
