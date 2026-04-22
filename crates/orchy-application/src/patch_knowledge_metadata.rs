use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore, Version};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct PatchKnowledgeMetadataCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub set: HashMap<String, String>,
    pub remove: Vec<String>,
    pub version: Option<u64>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

pub struct PatchKnowledgeMetadata {
    store: Arc<dyn KnowledgeStore>,
}

impl PatchKnowledgeMetadata {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: PatchKnowledgeMetadataCommand) -> Result<KnowledgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let path: KnowledgePath = cmd
            .path
            .parse()
            .map_err(|e: Error| Error::InvalidInput(e.to_string()))?;
        let expected_version = cmd.version.map(Version::new);

        let valid_from = cmd
            .valid_from
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid valid_from: {e}")))?;
        let valid_until = cmd
            .valid_until
            .map(|s| chrono::DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::InvalidInput(format!("invalid valid_until: {e}")))?;

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry: {path}")))?;

        if let Some(expected) = expected_version
            && entry.version() != expected
        {
            return Err(Error::VersionMismatch {
                expected: expected.as_u64(),
                actual: entry.version().as_u64(),
            });
        }

        if cmd.set.is_empty()
            && cmd.remove.is_empty()
            && valid_from.is_none()
            && valid_until.is_none()
        {
            return Ok(KnowledgeResponse::from(&entry));
        }

        entry.set_validity(valid_from, valid_until)?;

        for (k, v) in &cmd.set {
            entry.set_metadata(k.clone(), v.clone())?;
        }
        for k in &cmd.remove {
            entry.remove_metadata(k)?;
        }

        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
