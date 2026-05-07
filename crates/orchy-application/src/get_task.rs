use std::ops::Deref;
use std::sync::Arc;

use serde::Serialize;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::graph::Relation;
use orchy_core::graph::RelationOptions;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::{TaskId, TaskStore};

use crate::dto::TaskDto;
use crate::materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};

pub struct GetTaskCommand {
    pub task_id: String,
    pub org_id: String,
    pub relations: Option<RelationOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetTaskDto {
    #[serde(flatten)]
    pub task: TaskDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<Relation>>,
}

impl Deref for GetTaskDto {
    type Target = TaskDto;
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

    pub async fn execute(&self, cmd: GetTaskCommand) -> ApplicationResult<GetTaskDto> {
        let task_id = cmd.task_id.parse::<TaskId>()?;
        let org_id = OrganizationId::new(&cmd.org_id)?;

        let task = self
            .tasks
            .find_by_id(&task_id)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            })?;

        if task.org_id() != &org_id {
            return Err(Error::NotFound {
                resource: Resource::Task,
                id: task_id.to_string(),
            }
            .into());
        }

        let relations = if let (Some(opts), Some(mat)) = (cmd.relations, &self.materializer) {
            let neighborhood = mat
                .execute(MaterializeNeighborhoodCommand {
                    org_id: cmd.org_id,
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

        Ok(GetTaskDto {
            task: TaskDto::from(&task),
            relations,
        })
    }
}
