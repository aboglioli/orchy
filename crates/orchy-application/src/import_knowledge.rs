use std::str::FromStr;
use std::sync::Arc;

use orchy_core::agent::AgentId;
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

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
    pub agent_id: Option<String>,
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

    pub async fn execute(&self, cmd: ImportKnowledgeCommand) -> Result<Knowledge> {
        let source_org = OrganizationId::new(&cmd.source_org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let source_project = ProjectId::try_from(cmd.source_project)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let source_namespace = parse_namespace(cmd.source_namespace.as_deref())?;

        let source = self
            .store
            .find_by_path(
                &source_org,
                Some(&source_project),
                &source_namespace,
                &cmd.source_path,
            )
            .await?
            .ok_or_else(|| {
                Error::NotFound(format!("source knowledge entry: {}", cmd.source_path))
            })?;

        let target_org = OrganizationId::new(&cmd.target_org_id)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let target_project = ProjectId::try_from(cmd.target_project)
            .map_err(|e| Error::InvalidInput(e.to_string()))?;
        let target_namespace = parse_namespace(cmd.target_namespace.as_deref())?;
        let target_path = cmd.target_path.unwrap_or_else(|| source.path().to_string());

        let agent_id = cmd
            .agent_id
            .map(|s| AgentId::from_str(&s))
            .transpose()
            .map_err(Error::InvalidInput)?;

        let mut entry = Knowledge::new(
            target_org,
            Some(target_project),
            target_namespace,
            target_path,
            source.kind(),
            source.title().to_string(),
            source.content().to_string(),
            source.tags().to_vec(),
            agent_id,
            source.metadata().clone(),
        )?;

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }
}
