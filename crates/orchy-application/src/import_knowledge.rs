use std::sync::Arc;

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct ImportKnowledgeCommand {
    pub source_org_id: String,
    pub source_project: String,
    pub source_namespace: Option<String>,
    pub source_path: String,
    pub target_org_id: String,
    pub target_project: String,
    pub target_namespace: Option<String>,
    pub target_path: Option<String>,
}

pub struct ImportKnowledge {
    store: Arc<dyn KnowledgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
}

impl ImportKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    ) -> Self {
        Self { store, embeddings }
    }

    pub async fn execute(&self, cmd: ImportKnowledgeCommand) -> Result<KnowledgeResponse> {
        let source_org = OrganizationId::new(&cmd.source_org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let source_project = ProjectId::try_from(cmd.source_project)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let source_namespace = parse_namespace(cmd.source_namespace.as_deref())?;
        let source_path: KnowledgePath = cmd
            .source_path
            .parse::<KnowledgePath>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let source = self
            .store
            .find_by_path(
                &source_org,
                Some(&source_project),
                &source_namespace,
                &source_path,
            )
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("source knowledge entry: {source_path}"))
            })?;

        let target_org = OrganizationId::new(&cmd.target_org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let target_project = ProjectId::try_from(cmd.target_project)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let target_namespace = parse_namespace(cmd.target_namespace.as_deref())?;
        let target_path_str = cmd.target_path.unwrap_or_else(|| source.path().as_str().to_string());
        let target_path: KnowledgePath = target_path_str
            .parse::<KnowledgePath>()
            .map_err(|e| Error::InvalidInput(e.to_string()))?;

        let mut entry = Knowledge::new(
            target_org,
            Some(target_project),
            target_namespace,
            target_path,
            source.kind(),
            source.title().to_string(),
            source.content().to_string(),
            source.tags().to_vec(),
            source.metadata().clone(),
        )?;

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions())?;
        }

        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
