use std::sync::Arc;

use orchy_core::error::{Error, Result};
use orchy_core::knowledge::KnowledgeStore;
use orchy_core::namespace::ProjectId;
use orchy_core::organization::OrganizationId;
use orchy_core::resource_ref::{ResourceKind, ResourceRef};

use crate::parse_namespace;

use crate::dto::KnowledgeResponse;

pub struct AddKnowledgeRefCommand {
    pub org_id: String,
    pub project: String,
    pub namespace: Option<String>,
    pub path: String,
    pub ref_kind: String,
    pub ref_id: String,
    pub ref_display: Option<String>,
}

pub struct AddKnowledgeRef {
    store: Arc<dyn KnowledgeStore>,
}

impl AddKnowledgeRef {
    pub fn new(store: Arc<dyn KnowledgeStore>) -> Self {
        Self { store }
    }

    pub async fn execute(&self, cmd: AddKnowledgeRefCommand) -> Result<KnowledgeResponse> {
        let org_id =
            OrganizationId::new(&cmd.org_id).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let project =
            ProjectId::try_from(cmd.project).map_err(|e| Error::InvalidInput(e.to_string()))?;
        let namespace = parse_namespace(cmd.namespace.as_deref())?;

        let kind = cmd
            .ref_kind
            .parse::<ResourceKind>()
            .map_err(Error::InvalidInput)?;

        let mut r = ResourceRef::new(kind, cmd.ref_id);
        if let Some(d) = cmd.ref_display {
            r = r.with_display(d);
        }

        let mut entry = self
            .store
            .find_by_path(&org_id, Some(&project), &namespace, &cmd.path)
            .await?
            .ok_or_else(|| Error::NotFound(format!("knowledge entry: {}", cmd.path)))?;

        entry.add_ref(r);
        self.store.save(&mut entry).await?;
        Ok(KnowledgeResponse::from(&entry))
    }
}
