use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::graph::{Edge, EdgeStore, RelationType};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;

use crate::dto::KnowledgeResponse;
use crate::parse_namespace;

pub struct ConsolidateKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub source_paths: Vec<String>,
    pub target_path: String,
    pub target_title: String,
    pub target_kind: Option<String>,
}

pub struct ConsolidateKnowledge {
    knowledge: Arc<dyn KnowledgeStore>,
    edges: Arc<dyn EdgeStore>,
}

impl ConsolidateKnowledge {
    pub fn new(knowledge: Arc<dyn KnowledgeStore>, edges: Arc<dyn EdgeStore>) -> Self {
        Self { knowledge, edges }
    }

    pub async fn execute(&self, cmd: ConsolidateKnowledgeCommand) -> Result<KnowledgeResponse> {
        if cmd.source_paths.len() < 2 {
            return Err(Error::InvalidInput(
                "consolidate requires at least 2 source paths".to_string(),
            ));
        }

        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let kind = cmd
            .target_kind
            .as_deref()
            .unwrap_or("summary")
            .parse::<KnowledgeKind>()
            .map_err(Error::InvalidInput)?;

        let mut sources = Vec::new();
        for path_str in &cmd.source_paths {
            let path: KnowledgePath = path_str
                .parse::<KnowledgePath>()
                .map_err(|e| Error::InvalidInput(e.to_string()))?;
            let entry = self
                .knowledge
                .find_by_path(&org_id, Some(&project), &namespace, &path)
                .await?
                .ok_or_else(|| Error::NotFound(format!("knowledge {path_str}")))?;
            sources.push(entry);
        }

        let mut content = String::new();
        for (i, src) in sources.iter().enumerate() {
            if i > 0 {
                content.push_str("\n\n---\n\n");
            }
            content.push_str(&format!("# {} ({})", src.title(), src.path()));
            content.push('\n');
            content.push_str(src.content());
        }

        let target_path: KnowledgePath = cmd
            .target_path
            .parse::<KnowledgePath>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let mut consolidated = Knowledge::new(
            org_id.clone(),
            Some(project.clone()),
            namespace.clone(),
            target_path,
            kind,
            cmd.target_title,
            content,
            Vec::new(),
            Default::default(),
        )?;

        self.knowledge.save(&mut consolidated).await?;

        for src in &sources {
            let mut edge = Edge::new(
                org_id.clone(),
                ResourceKind::Knowledge,
                consolidated.id().to_string(),
                ResourceKind::Knowledge,
                src.id().to_string(),
                RelationType::MergedFrom,
                None,
            )?;
            let _ = self.edges.save(&mut edge).await;
        }

        for src in sources {
            let _ = self.knowledge.delete(&src.id()).await;
        }

        Ok(KnowledgeResponse::from(&consolidated))
    }
}
