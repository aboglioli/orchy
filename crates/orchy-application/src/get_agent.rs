use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use serde::Serialize;

use orchy_core::agent::{AgentId, AgentStore};
use orchy_core::error::{Error, Result};
use orchy_core::graph::Relation;
use orchy_core::graph::RelationOptions;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::AgentResponse;
use crate::materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};

pub struct GetAgentCommand {
    pub agent_id: String,
    pub org_id: Option<String>,
    pub relations: Option<RelationOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetAgentResponse {
    #[serde(flatten)]
    pub agent: AgentResponse,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<Relation>>,
}

impl Deref for GetAgentResponse {
    type Target = AgentResponse;
    fn deref(&self) -> &Self::Target {
        &self.agent
    }
}

pub struct GetAgent {
    agents: Arc<dyn AgentStore>,
    materializer: Option<Arc<MaterializeNeighborhood>>,
}

impl GetAgent {
    pub fn new(
        agents: Arc<dyn AgentStore>,
        materializer: Option<Arc<MaterializeNeighborhood>>,
    ) -> Self {
        Self {
            agents,
            materializer,
        }
    }

    pub async fn execute(&self, cmd: GetAgentCommand) -> Result<GetAgentResponse> {
        let id = AgentId::from_str(&cmd.agent_id)?;
        let agent = self
            .agents
            .find_by_id(&id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent {id}")))?;

        let relations = if let (Some(opts), Some(mat), Some(org_id)) =
            (cmd.relations, &self.materializer, cmd.org_id)
        {
            let neighborhood = mat
                .execute(MaterializeNeighborhoodCommand {
                    org_id,
                    anchor_kind: ResourceKind::Agent.to_string(),
                    anchor_id: id.to_string(),
                    options: opts,
                    as_of: None,
                    project: None,
                    namespace: None,
                    semantic_query: None,
                })
                .await?;
            Some(neighborhood.relations)
        } else {
            None
        };

        Ok(GetAgentResponse {
            agent: AgentResponse::from(agent),
            relations,
        })
    }
}
