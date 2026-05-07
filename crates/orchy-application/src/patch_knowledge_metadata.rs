use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

use crate::error::ApplicationResult;
use orchy_core::error::{Error, Resource};
use orchy_core::knowledge::{KnowledgePath, KnowledgeStore, Version};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::dto::KnowledgeDto;
use crate::parse_namespace;

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

    pub async fn execute(
        &self,
        cmd: PatchKnowledgeMetadataCommand,
    ) -> ApplicationResult<KnowledgeDto> {
        let org_id = OrganizationId::new(&cmd.org_id)?;
        let project = ProjectId::try_from(cmd.project)?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
        let path: KnowledgePath = cmd.path.parse()?;
        let expected_version = cmd.version.map(Version::new);

        let valid_from = cmd
            .valid_from
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::invalid_input(format!("invalid valid_from: {e}")))?;
        let valid_until = cmd
            .valid_until
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()
            .map_err(|e| Error::invalid_input(format!("invalid valid_until: {e}")))?;

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

        if cmd.set.is_empty()
            && cmd.remove.is_empty()
            && valid_from.is_none()
            && valid_until.is_none()
        {
            return Ok(KnowledgeDto::from(&entry));
        }

        entry.set_validity(valid_from, valid_until)?;

        for (k, v) in &cmd.set {
            entry.set_metadata(k.clone(), v.clone())?;
        }
        for k in &cmd.remove {
            entry.remove_metadata(k)?;
        }

        self.store.save(&mut entry).await?;
        Ok(KnowledgeDto::from(&entry))
    }
}
