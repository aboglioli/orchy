use std::sync::Arc;

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::organization::OrganizationId;

use orchy_core::namespace::ProjectId;

use crate::dto::KnowledgeResponse;
use crate::parse_namespace;

pub struct SearchKnowledgeCommand {
    pub org_id: String,
    pub query: String,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<u32>,
    pub project: Option<String>,
}

pub struct SearchKnowledge {
    store: Arc<dyn KnowledgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
}

impl SearchKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    ) -> Self {
        Self { store, embeddings }
    }

    pub async fn execute(&self, cmd: SearchKnowledgeCommand) -> Result<Vec<KnowledgeResponse>> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;

        let namespace = cmd
            .namespace
            .as_deref()
            .map(|s| parse_namespace(Some(s)))
            .transpose()?;

        let limit = cmd.limit.unwrap_or(20) as usize;

        let embedding = if let Some(emb) = &self.embeddings {
            Some(emb.embed(&cmd.query).await?)
        } else {
            None
        };

        let project = cmd
            .project
            .map(|s| ProjectId::try_from(s).map_err(|e| Error::InvalidInput(e.to_string())))
            .transpose()?;

        let entries = self
            .store
            .search(
                &org_id,
                &cmd.query,
                embedding.as_deref(),
                namespace.as_ref(),
                limit,
            )
            .await?;

        let filtered: Vec<_> = if let Some(ref pid) = project {
            entries
                .iter()
                .filter(|e| e.project().map(|p| p == pid).unwrap_or(false))
                .map(KnowledgeResponse::from)
                .collect()
        } else {
            entries.iter().map(KnowledgeResponse::from).collect()
        };

        Ok(filtered)
    }
}
