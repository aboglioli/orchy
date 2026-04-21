use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use orchy_core::agent::AgentId;
use orchy_core::edge::{Edge, EdgeStore, RelationType};
use orchy_core::embeddings::EmbeddingsProvider;
use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{
    Knowledge, KnowledgeKind, KnowledgePath, KnowledgeStore, Version,
    WriteKnowledge as WriteKnowledgeCmd,
};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::ResourceKind;
use orchy_core::task::TaskId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct WriteKnowledgeCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub kind: String,
    pub title: String,
    pub content: String,
    pub tags: Option<Vec<String>>,
    pub version: Option<u64>,
    pub agent_id: Option<String>,
    pub metadata: Option<HashMap<String, String>>,
    pub metadata_remove: Option<Vec<String>>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    /// If set, auto-creates a Task→Knowledge Produces edge after writing.
    pub task_id: Option<String>,
}

pub struct WriteKnowledge {
    store: Arc<dyn KnowledgeStore>,
    edges: Arc<dyn EdgeStore>,
    embeddings: Option<Arc<dyn EmbeddingsProvider>>,
}

impl WriteKnowledge {
    pub fn new(
        store: Arc<dyn KnowledgeStore>,
        edges: Arc<dyn EdgeStore>,
        embeddings: Option<Arc<dyn EmbeddingsProvider>>,
    ) -> Self {
        Self {
            store,
            edges,
            embeddings,
        }
    }

    pub async fn execute(&self, cmd: WriteKnowledgeCommand) -> Result<KnowledgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let kind = cmd
            .kind
            .parse::<KnowledgeKind>()
            .map_err(Error::InvalidInput)?;
        let agent_id = cmd.agent_id.map(|s| AgentId::from_str(&s)).transpose()?;
        let expected_version = cmd.version.map(Version::new);
        let task_id = cmd.task_id.as_deref().map(TaskId::from_str).transpose()?;
        let has_valid_from = cmd.valid_from.is_some();
        let valid_from = cmd
            .valid_from
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid valid_from: {e}")))?;
        let has_valid_until = cmd.valid_until.is_some();
        let valid_until = cmd
            .valid_until
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid valid_until: {e}")))?;

        let write_cmd = WriteKnowledgeCmd {
            org_id: org_id.clone(),
            project: Some(project),
            namespace,
            path: KnowledgePath::new(&cmd.path).map_err(|e| Error::InvalidInput(e.to_string()))?,
            kind,
            title: cmd.title,
            content: cmd.content,
            tags: cmd.tags.unwrap_or_default(),
            expected_version,
            metadata: cmd.metadata.unwrap_or_default(),
            metadata_remove: cmd.metadata_remove.unwrap_or_default(),
            valid_from,
            valid_until,
        };

        let existing = self
            .store
            .find_by_path(
                &write_cmd.org_id,
                write_cmd.project.as_ref(),
                &write_cmd.namespace,
                &write_cmd.path,
            )
            .await?;

        let mut entry = if let Some(mut existing) = existing {
            if let Some(expected) = write_cmd
                .expected_version
                .filter(|v| existing.version() != *v)
            {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: existing.version().as_u64(),
                });
            }
            existing.update(write_cmd.title, write_cmd.content)?;
            for tag in &write_cmd.tags {
                existing.add_tag(tag.clone())?;
            }
            for k in &write_cmd.metadata_remove {
                existing.remove_metadata(k)?;
            }
            for (k, v) in &write_cmd.metadata {
                existing.set_metadata(k.clone(), v.clone())?;
            }
            if has_valid_from || has_valid_until {
                existing.set_validity(
                    if has_valid_from {
                        write_cmd.valid_from
                    } else {
                        existing.valid_from()
                    },
                    if has_valid_until {
                        write_cmd.valid_until
                    } else {
                        existing.valid_until()
                    },
                )?;
            }
            existing
        } else {
            if let Some(expected) = write_cmd.expected_version {
                return Err(Error::VersionMismatch {
                    expected: expected.as_u64(),
                    actual: 0,
                });
            }
            let mut created = Knowledge::new(
                write_cmd.org_id,
                write_cmd.project,
                write_cmd.namespace,
                write_cmd.path,
                write_cmd.kind,
                write_cmd.title,
                write_cmd.content,
                write_cmd.tags,
                write_cmd.metadata,
            )?;
            for k in &write_cmd.metadata_remove {
                created.remove_metadata(k)?;
            }
            created.set_validity(write_cmd.valid_from, write_cmd.valid_until)?;
            created
        };

        if let Some(emb) = &self.embeddings {
            let text = format!("{} {}", entry.title(), entry.content());
            let vector = emb.embed(&text).await?;
            entry.set_embedding(vector, emb.model().to_string(), emb.dimensions())?;
        }

        self.store.save(&mut entry).await?;

        if let Some(task_id) = task_id {
            let task_id_str = task_id.to_string();
            let edge_exists = self
                .edges
                .exists_by_pair(
                    &org_id,
                    &ResourceKind::Task,
                    &task_id_str,
                    &ResourceKind::Knowledge,
                    &entry.id().to_string(),
                    &RelationType::Produces,
                )
                .await
                .unwrap_or(false);

            if !edge_exists {
                let mut edge = match Edge::new(
                    org_id,
                    ResourceKind::Task,
                    task_id_str.clone(),
                    ResourceKind::Knowledge,
                    entry.id().to_string(),
                    RelationType::Produces,
                    agent_id,
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("failed to create edge: {e}");
                        return Ok(KnowledgeResponse::from(&entry));
                    }
                }
                .with_source(ResourceKind::Task, task_id_str);
                if let Err(e) = self.edges.save(&mut edge).await {
                    tracing::warn!(
                        "failed to create produces edge for knowledge {}: {e}",
                        entry.id()
                    );
                }
            }
        }

        Ok(KnowledgeResponse::from(&entry))
    }
}
