use std::sync::Arc;

use crate::embeddings::EmbeddingsProvider;
use crate::error::ApplicationResult;
use orchy_core::embeddings::Embedding;
use orchy_core::error::{Error, Resource};
use orchy_core::knowledge::{Knowledge, KnowledgePath, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::KnowledgeDto;
use crate::parse_namespace;

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

    pub async fn execute(&self, cmd: ImportKnowledgeCommand) -> ApplicationResult<KnowledgeDto> {
        let source_org = OrganizationId::new(&cmd.source_org_id)?;
        let source_project = ProjectId::try_from(cmd.source_project)?;
        let source_namespace = parse_namespace(cmd.source_namespace.as_deref())?;
        let source_path: KnowledgePath = cmd.source_path.parse::<KnowledgePath>()?;

        let source = self
            .store
            .find_by_path(
                &source_org,
                Some(&source_project),
                &source_namespace,
                &source_path,
            )
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Knowledge,
                id: source_path.to_string(),
            })?;

        let target_org = OrganizationId::new(&cmd.target_org_id)?;
        let target_project = ProjectId::try_from(cmd.target_project)?;
        let target_namespace = parse_namespace(cmd.target_namespace.as_deref())?;
        let target_path_str = cmd
            .target_path
            .unwrap_or_else(|| source.path().as_str().to_owned());
        let target_path: KnowledgePath = target_path_str.parse::<KnowledgePath>()?;

        let mut entry = Knowledge::new(
            target_org,
            Some(target_project),
            target_namespace,
            target_path,
            source.kind(),
            source.title().to_owned(),
            source.content().to_owned(),
            source.tags().to_vec(),
            source.metadata().clone(),
        )?;

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            let embedding = Embedding::new(vector, emb.model().to_owned(), emb.dimensions())?;
            entry.set_embedding(embedding)?;
        }

        self.store.save(&mut entry).await?;
        Ok(KnowledgeDto::from(&entry))
    }
}
