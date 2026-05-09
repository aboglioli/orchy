use std::sync::Arc;

use crate::embeddings::EmbeddingsProvider;
use crate::error::ApplicationResult;
use orchy_core::embeddings::Embedding;
use orchy_core::error::{Error, Resource};
use orchy_core::knowledge::{KnowledgeKind, KnowledgePath, KnowledgeStore, Version};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::KnowledgeDto;
use crate::parse_namespace;

pub struct ChangeKnowledgeKindCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub new_kind: String,
    pub version: Option<u64>,
}

pub struct ChangeKnowledgeKind {
    store: Arc<dyn KnowledgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
}

impl ChangeKnowledgeKind {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    ) -> Self {
        Self { store, embeddings }
    }

    pub async fn execute(
        &self,
        cmd: ChangeKnowledgeKindCommand,
    ) -> ApplicationResult<KnowledgeDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let new_kind = cmd.new_kind.parse::<KnowledgeKind>()?;
        let path: KnowledgePath = cmd.path.parse::<KnowledgePath>()?;
        let expected_version = cmd.version.map(Version::new);

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &path)
            .await?
            .ok_or_else(|| Error::NotFound {
                resource: Resource::Knowledge,
                id: path.to_string(),
            })?;

        if let Some(expected) = expected_version
            && entry.version() != expected
        {
            return Err(
                Error::version_mismatch(expected.as_u64(), entry.version().as_u64()).into(),
            );
        }

        if entry.kind() == new_kind {
            self.store.save(&mut entry).await?;
            return Ok(KnowledgeDto::from(&entry));
        }

        entry.change_kind(new_kind)?;

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
