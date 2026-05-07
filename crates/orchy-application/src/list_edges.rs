use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::error::ApplicationResult;
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

    pub async fn execute(&self, cmd: ListEdgesCommand) -> ApplicationResult<PageResponse<EdgeDto>> {
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let rel_type = cmd
            .rel_type
            .as_deref()
            .map(|s| s.parse::<RelationType>())
            .transpose()?;

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
