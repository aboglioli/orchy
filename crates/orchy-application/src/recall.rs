use std::sync::Arc;

use orchy_core::{
    Clock, DocumentStatus, EntityKind, Kind, Namespace, Search, SearchQuery, Tag, rank,
    within_budget,
};
use serde::{Deserialize, Serialize};

use crate::dto::HitDto;
use crate::error::ApplicationResult;

const DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecallCommand {
    pub text: String,
    pub entities: Vec<String>,
    pub kind: Vec<String>,
    pub retired: bool,
    pub status: Vec<String>,
    pub namespace: Option<String>,
    pub anchor: Option<String>,
    pub tags: Vec<String>,
    pub limit: Option<usize>,
    pub budget: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallDto {
    pub hits: Vec<HitDto>,
    pub total: usize,
}

pub struct Recall {
    search: Arc<dyn Search>,
    clock: Arc<dyn Clock>,
}

impl Recall {
    pub fn new(search: Arc<dyn Search>, clock: Arc<dyn Clock>) -> Self {
        Self { search, clock }
    }

    pub async fn execute(&self, cmd: RecallCommand) -> ApplicationResult<RecallDto> {
        let limit = cmd.limit.unwrap_or(DEFAULT_LIMIT);
        let query = SearchQuery {
            text: cmd.text,
            entities: if cmd.entities.is_empty() {
                None
            } else {
                Some(
                    cmd.entities
                        .iter()
                        .map(|e| e.parse::<EntityKind>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            retired: cmd.retired,
            kind: if cmd.kind.is_empty() {
                None
            } else {
                Some(
                    cmd.kind
                        .iter()
                        .map(|k| k.parse::<Kind>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            exclude_status: if cmd.status.is_empty() {
                DocumentStatus::RETIRED.to_vec()
            } else {
                Vec::new()
            },
            status: if cmd.status.is_empty() {
                None
            } else {
                Some(
                    cmd.status
                        .iter()
                        .map(|s| s.parse::<DocumentStatus>())
                        .collect::<orchy_core::Result<Vec<_>>>()?,
                )
            },
            namespace: cmd.namespace.as_deref().map(Namespace::new).transpose()?,
            tags: cmd
                .tags
                .iter()
                .map(Tag::new)
                .collect::<orchy_core::Result<Vec<_>>>()?,
            since: None,
            limit,
        };

        let anchor = cmd.anchor.as_deref().map(Namespace::new).transpose()?;
        let mut hits = self.search.sections(&query).await?;
        rank(&mut hits, anchor.as_ref(), self.clock.now());
        let total = hits.len();
        hits.truncate(limit);

        let hits = match cmd.budget {
            None => hits.iter().map(HitDto::from).collect(),
            Some(tokens) => within_budget(hits, tokens)
                .iter()
                .map(|hit| HitDto {
                    text: Some(hit.body.trim().to_owned()),
                    ..HitDto::from(hit)
                })
                .collect(),
        };
        Ok(RecallDto { hits, total })
    }
}
