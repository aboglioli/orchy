use std::sync::Arc;

use serde::Serialize;

use orchy_core::error::{Error, Result};
use orchy_core::graph::Relation;
use orchy_core::graph::RelationOptions;
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::materialize_neighborhood::{MaterializeNeighborhood, MaterializeNeighborhoodCommand};
use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct ReadKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub relations: Option<RelationOptions>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadKnowledgeResponse {
    pub knowledge: Option<KnowledgeResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relations: Option<Vec<Relation>>,
}

pub struct ReadKnowledge {
    store: Arc<dyn KnowledgeStore>,
    materializer: Option<Arc<MaterializeNeighborhood>>,
}

impl ReadKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        materializer: Option<Arc<MaterializeNeighborhood>>,
    ) -> Self {
        Self {
            store,
            materializer,
        }
    }

    pub async fn execute(&self, cmd: ReadKnowledgeCommand) -> Result<ReadKnowledgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let path: KnowledgePath = cmd
            .path
            .parse::<KnowledgePath>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &path)
            .await?;

        let relations =
            if let (Some(k), Some(opts), Some(mat)) = (&entry, cmd.relations, &self.materializer) {
                // Try both UUID and path as anchor — edges may reference knowledge by either
                let uuid_anchor = k.id().to_string();
                let path_anchor = k.path().to_string();
                let project = k.project().map(|p| p.to_string());

                let mut n = mat
                    .execute(MaterializeNeighborhoodCommand {
                        org_id: cmd.org_id.clone(),
                        anchor_kind: ResourceKind::Knowledge.to_string(),
                        anchor_id: uuid_anchor.clone(),
                        options: opts.clone(),
                        as_of: None,
                        project: project.clone(),
                        namespace: None,
                        semantic_query: None,
                    })
                    .await?;

                if path_anchor != uuid_anchor {
                    let path_n = mat
                        .execute(MaterializeNeighborhoodCommand {
                            org_id: cmd.org_id,
                            anchor_kind: ResourceKind::Knowledge.to_string(),
                            anchor_id: path_anchor,
                            options: opts,
                            as_of: None,
                            project,
                            namespace: None,
                            semantic_query: None,
                        })
                        .await?;
                    n.relations.extend(path_n.relations);
                }

                Some(n.relations)
            } else {
                None
            };

        Ok(ReadKnowledgeResponse {
            knowledge: entry.map(|e| KnowledgeResponse::from(&e)),
            relations,
        })
    }
}
