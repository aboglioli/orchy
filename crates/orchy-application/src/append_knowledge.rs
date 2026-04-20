use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeKind, KnowledgeStore};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct AppendKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub kind: String,
    pub value: String,
    pub separator: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub metadata_remove: Option<Vec<String>>,
}

pub struct AppendKnowledge {
    store: Arc<dyn KnowledgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
}

impl AppendKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    ) -> Self {
        Self { store, embeddings }
    }

    pub async fn execute(&self, cmd: AppendKnowledgeCommand) -> Result<KnowledgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let kind = cmd
            .kind
            .parse::<KnowledgeKind>()
            .map_err(Error::InvalidInput)?;
        let separator = cmd.separator.as_deref().unwrap_or("\n");

        let existing = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.path)
            .await?;

        let mut entry = if let Some(mut existing) = existing {
            let new_content = format!("{}{}{}", existing.content(), separator, cmd.value);
            existing.update(existing.title().to_string(), new_content)?;
            existing
        } else {
            let title = cmd.path.clone();
            Knowledge::new(
                org_id,
                Some(project),
                namespace,
                cmd.path,
                kind,
                title,
                cmd.value,
                vec![],
                HashMap::new(),
            )?
        };

        if let Some(set) = cmd.metadata {
            for (k, v) in set {
                entry.set_metadata(k, v)?;
            }
        }
        if let Some(remove) = cmd.metadata_remove {
            for k in remove {
                entry.remove_metadata(&k)?;
            }
        }

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions())?;
        }

        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
