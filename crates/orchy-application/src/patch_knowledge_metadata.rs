use std::collections::HashMap;
use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::{Knowledge, KnowledgeStore, Version};
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;

use crate::parse_namespace;

pub struct PatchKnowledgeMetadataCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub set: HashMap<String, String>,
    pub remove: Vec<String>,
    pub version: Option<u64>,
}

pub struct PatchKnowledgeMetadata {
    store: Arc<dyn KnowledgeStore>,
}

impl PatchKnowledgeMetadata {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: PatchKnowledgeMetadataCommand) -> Result<Knowledge> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;
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

        if cmd.set.is_empty() && cmd.remove.is_empty() {
            return Ok(entry);
        }

        for k in &cmd.remove {
            entry.remove_metadata(k)?;
        }
        for (k, v) in cmd.set {
            entry.set_metadata(k, v)?;
        }

        self.store.save(&mut entry).await?;
        Ok(entry)
    }
}
