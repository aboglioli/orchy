use std::ops::Deref;
use std::sync::Arc;

use serde::Serialize;

use orchy_core::error::{Error, Result};
use orchy_core::graph::Relation;
use orchy_core::graph::RelationOptions;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskResponse;
use crate::materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};

pub struct GetTaskCommand {
    pub task_id: String,
    pub org_id: Option<String>,
    pub relations: Option<RelationOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetTaskResponse {
    #[serde(flatten)]
    pub task: TaskResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<Relation>>,
}

impl Deref for GetTaskResponse {
    type Target = TaskResponse;
    fn deref(&self) -> &Self::Target {
        &self.task
    }
}

pub struct GetTask {
    tasks: Arc<dyn TaskStore>,
    materializer: Option<Arc<MaterializeNeighborhood>>,
}

impl GetTask {
    pub fn new(
        tasks: Arc<dyn TaskStore>,
        materializer: Option<Arc<MaterializeNeighborhood>>,
    ) -> Self {
        Self {
            tasks,
            materializer,
        }
    }

    pub async fn execute(&self, cmd: GetTaskCommand) -> Result<GetTaskResponse> {
        let task_id = cmd.task_id.parse::<TaskId>()?;

        let task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("task {task_id}")))?;

        let relations = if let (Some(opts), Some(mat), Some(org_id)) =
            (cmd.relations, &self.materializer, cmd.org_id)
        {
            let neighborhood = mat
                .execute(MaterializeNeighborhoodCommand {
                    org_id,
                    anchor_kind: ResourceKind::Task.to_string(),
                    anchor_id: task_id.to_string(),
                    options: opts,
                    as_of: None,
                    project: Some(task.project().to_string()),
                    namespace: None,
                    semantic_query: None,
                })
                .await?;
            Some(neighborhood.relations)
        } else {
            None
        };

        Ok(GetTaskResponse {
            task: TaskResponse::from(&task),
            relations,
        })
    }
}
