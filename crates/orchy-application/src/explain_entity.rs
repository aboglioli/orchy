use std::sync::Arc;

use orchy_core::{EdgeStore, EntityRef, EventLog, EventQuery};
use serde::{Deserialize, Serialize};

use crate::dto::{EdgeDto, EventDto};
use crate::error::ApplicationResult;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExplainEntityCommand {
    pub entity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplanationDto {
    pub entity: String,
    pub history: Vec<EventDto>,
    pub links_out: Vec<EdgeDto>,
    pub links_in: Vec<EdgeDto>,
}

pub struct ExplainEntity {
    log: Arc<dyn EventLog>,
    edges: Arc<dyn EdgeStore>,
}

impl ExplainEntity {
    pub fn new(log: Arc<dyn EventLog>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { log, edges }
    }

    pub async fn execute(&self, cmd: ExplainEntityCommand) -> ApplicationResult<ExplanationDto> {
        let entity: EntityRef = cmd.entity.parse()?;
        let history = self
            .log
            .replay(&EventQuery {
                key: Some(entity.id().clone()),
                ..Default::default()
            })
            .await?;
        let links_out = self.edges.out(&entity, None).await?;
        let links_in = self.edges.incoming(&entity, None).await?;
        Ok(ExplanationDto {
            entity: entity.to_string(),
            history: history.iter().map(EventDto::from).collect(),
            links_out: links_out.iter().map(EdgeDto::from).collect(),
            links_in: links_in.iter().map(EdgeDto::from).collect(),
        })
    }
}
