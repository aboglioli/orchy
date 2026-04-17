use std::sync::Arc;

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore, Version};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

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

    pub async fn execute(&self, cmd: ChangeKnowledgeKindCommand) -> Result<Knowledge> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let new_kind = cmd
            .new_kind
            .parse::<KnowledgeKind>()
            .map_err(Error::InvalidInput)?;
        let expected_version = cmd.version.map(Version::from);

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry: {}", cmd.path)))?;

        if let Some(expected) = expected_version
            && entry.version() != expected
        {
            return Err(Error::VersionMismatch {
                expected: expected.as_u64(),
                actual: entry.version().as_u64(),
            });
        }

        if entry.kind() == new_kind {
            self.store.save(&mut entry).await?;
            return Ok(entry);
        }

        entry.change_kind(new_kind)?;

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions());
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }
}
