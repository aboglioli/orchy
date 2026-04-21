use std::sync::Arc;

use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::{Namespace, ProjectId};
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::KnowledgeResponse;
use crate::parse_namespace;

pub struct PromoteKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub source_path: String,
    pub target_path: String,
    pub target_title: Option<String>,
    pub instruction: Option<String>,
}

pub struct PromoteKnowledge {
    knowledge: Arc<dyn KnowledgeStore>,
    edges: Arc<dyn EdgeStore>,
}

impl PromoteKnowledge {
    pub fn new(knowledge: Arc<dyn KnowledgeStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { knowledge, edges }
    }

    pub async fn execute(&self,
        cmd: PromoteKnowledgeCommand,
    ) -> Result<KnowledgeResponse> {
        let org_id = OrganizationId::new(&cmd.org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project = ProjectId::try_from(cmd.project)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let source = self
            .knowledge
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.source_path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge {}", cmd.source_path)))?;

        match source.kind() {
            KnowledgeKind::Decision | KnowledgeKind::Discovery | KnowledgeKind::Pattern => {}
            other => {
                return Err(Error::InvalidInput(format!(
                    "cannot promote kind '{}': only decision, discovery, or pattern can be promoted",
                    other
                )));
            }
        }

        let title = cmd.target_title.unwrap_or_else(|| {
            format!("Skill: {}", source.title())
        });
        let content = if let Some(inst) = cmd.instruction {
            format!("{}\n\n## Source\n\n{}", inst, source.content())
        } else {
            source.content().to_string()
        };

        let mut promoted = Knowledge::new(
            org_id.clone(),
            Some(project.clone()),
            namespace.clone(),
            cmd.target_path,
            KnowledgeKind::Skill,
            title,
            content,
            source.tags().to_vec(),
            source.metadata().clone(),
        )?;

        self.knowledge.save(&mut promoted).await?;

        let mut edge = Edge::new(
            org_id.clone(),
            ResourceKind::Knowledge,
            promoted.id().to_string(),
            ResourceKind::Knowledge,
            source.id().to_string(),
            RelationType::DerivedFrom,
            None,
        )?;
        let _ = self.edges.save(&mut edge).await;

        Ok(KnowledgeResponse::from(&promoted))
    }
}
